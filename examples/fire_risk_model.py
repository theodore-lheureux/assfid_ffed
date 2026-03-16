#!/usr/bin/env python3
import argparse
import datetime as dt
import io
import json
import uuid
from pathlib import Path

import numpy as np
import pika
import torch
import torch.nn as nn
import torch.nn.functional as F
import matplotlib.pyplot as plt
import matplotlib.colors as mcolors

PIPELINE_DEFAULTS = {
    "url":           "amqp://ffed:changeme@jetson.local:5672/%2f",
    "tiff_exchange": "tiff_queue",
    "out_exchange":  "model_data_queue",
}

SENSOR_PROFILES: dict = {
    "full": {
        "description": "All spectral indices (NDVI + NDRE + RRI2)",
        "valid_indices": ["NDVI", "NDRE", "RRI2"],
        "mask": [1, 1, 1],
    },
    "a7sii": {
        "description": "Sony A7s II (R/G/B only) — NDVI only, no RedEdge",
        "valid_indices": ["NDVI"],
        "mask": [1, 0, 0],
    },
    "sentinel2": {
        "description": "Sentinel-2 MSI — full indices",
        "valid_indices": ["NDVI", "NDRE", "RRI2"],
        "mask": [1, 1, 1],
    },
    "landsat8": {
        "description": "Landsat-8 OLI — NDVI only (no RedEdge band)",
        "valid_indices": ["NDVI"],
        "mask": [1, 0, 0],
    },
    "micasense": {
        "description": "MicaSense RedEdge — full indices",
        "valid_indices": ["NDVI", "NDRE", "RRI2"],
        "mask": [1, 1, 1],
    },
}

INDEX_NAMES = ["NDVI", "NDRE", "RRI2"]


class IndicesLayer(nn.Module):
    EPS = 1e-6

    def __init__(self, red_idx, nir_idx,
                 rededge_idx=None, sensor_mask=None):
        super().__init__()
        self.red_idx     = red_idx
        self.nir_idx     = nir_idx
        self.rededge_idx = rededge_idx
        self.n_out       = 3
        # sensor_mask: 3 floats matching [NDVI, NDRE, RRI2] (1.0=keep, 0.0=zero)
        mask = sensor_mask if sensor_mask is not None else [1.0, 1.0, 1.0]
        self.register_buffer(
            "sensor_mask",
            torch.tensor(mask, dtype=torch.float32).view(1, 3, 1, 1)
        )

    def _norm_diff(self, a, b):
        return (a - b) / (a + b + self.EPS)

    def _ndvi(self, red, nir):
        """NDVI = (NIR - Red) / (NIR + Red)"""
        return self._norm_diff(nir, red)

    def _ndre(self, rededge, nir):
        """NDRE = (NIR - RedEdge) / (NIR + RedEdge)"""
        return self._norm_diff(nir, rededge)

    def _rri2(self, red, rededge):
        """RRI2 = RedEdge / Red — normalised to [0, 1] (max ratio assumed 5).
        High = healthy canopy. Low = stressed / burned vegetation."""
        return torch.clamp(rededge / (red + self.EPS), 0.0, 5.0) / 5.0

    def forward(self, x):
        red = x[:, self.red_idx:self.red_idx+1]
        nir = x[:, self.nir_idx:self.nir_idx+1]

        ndvi = self._ndvi(red, nir)

        if self.rededge_idx is not None:
            rededge = x[:, self.rededge_idx:self.rededge_idx+1]
            ndre = self._ndre(rededge, nir)
            rri2 = self._rri2(red, rededge)
        else:
            ndre = torch.zeros_like(ndvi)
            rri2 = torch.zeros_like(ndvi)

        # Stack: NDVI(0) NDRE(1) RRI2(2)
        indices = torch.cat([ndvi, ndre, rri2], dim=1)
        return indices * self.sensor_mask


class _ConvBNReLU(nn.Sequential):
    def __init__(self, in_ch: int, out_ch: int, kernel: int = 3, padding: int = 1):
        super().__init__(
            nn.Conv2d(in_ch, out_ch, kernel, padding=padding, bias=False),
            nn.BatchNorm2d(out_ch),
            nn.ReLU(inplace=True),
        )


class FireRiskCNN(nn.Module):
    def __init__(self, in_channels: int = 3, base_ch: int = 32):
        super().__init__()
        self.enc1 = nn.Sequential(_ConvBNReLU(in_channels, base_ch), _ConvBNReLU(base_ch, base_ch))
        self.enc2 = nn.Sequential(nn.MaxPool2d(2), _ConvBNReLU(base_ch, base_ch * 2), _ConvBNReLU(base_ch * 2, base_ch * 2))
        self.enc3 = nn.Sequential(nn.MaxPool2d(2), _ConvBNReLU(base_ch * 2, base_ch * 4), _ConvBNReLU(base_ch * 4, base_ch * 4))
        self.bottleneck = nn.Sequential(
            _ConvBNReLU(base_ch * 4, base_ch * 4, padding=2),
            nn.Conv2d(base_ch * 4, base_ch * 4, 3, padding=4, dilation=4, bias=False),
            nn.BatchNorm2d(base_ch * 4),
            nn.ReLU(inplace=True),
        )
        self.up2 = nn.ConvTranspose2d(base_ch * 4, base_ch * 2, 2, stride=2)
        self.dec2 = nn.Sequential(_ConvBNReLU(base_ch * 4, base_ch * 2), _ConvBNReLU(base_ch * 2, base_ch * 2))
        self.up1 = nn.ConvTranspose2d(base_ch * 2, base_ch, 2, stride=2)
        self.dec1 = nn.Sequential(_ConvBNReLU(base_ch * 2, base_ch), _ConvBNReLU(base_ch, base_ch))
        self.head = nn.Sequential(nn.Conv2d(base_ch, 16, 1), nn.ReLU(inplace=True), nn.Conv2d(16, 1, 1), nn.Sigmoid())

    @staticmethod
    def _pad_to_match(decoder: torch.Tensor, encoder: torch.Tensor) -> torch.Tensor:
        diff_h = encoder.shape[2] - decoder.shape[2]
        diff_w = encoder.shape[3] - decoder.shape[3]
        if diff_h != 0 or diff_w != 0:
            decoder = F.pad(decoder, [0, diff_w, 0, diff_h])
        return decoder

    def forward(self, x: torch.Tensor) -> torch.Tensor:
        e1 = self.enc1(x)
        e2 = self.enc2(e1)
        e3 = self.enc3(e2)
        b = self.bottleneck(e3)
        d2 = self._pad_to_match(self.up2(b), e2)
        d2 = self.dec2(torch.cat([d2, e2], dim=1))
        d1 = self._pad_to_match(self.up1(d2), e1)
        d1 = self.dec1(torch.cat([d1, e1], dim=1))
        return self.head(d1)


class FireRiskModel(nn.Module):
    """
    End-to-end model: raw spectral bands -> fire risk map.

        raw bands (B, C, H, W)  ->  IndicesLayer  ->  indices (B, 3, H, W)
                                ->  FireRiskCNN   ->  risk map (B, 1, H, W) in [0,1]
    """

    LOW_THRESHOLD    = 0.3
    MEDIUM_THRESHOLD = 0.6

    def __init__(self, red_idx=0, nir_idx=1,
                 rededge_idx=None, base_ch=32, sensor_profile="full"):
        super().__init__()
        profile = SENSOR_PROFILES.get(sensor_profile, SENSOR_PROFILES["full"])
        sensor_mask = [float(v) for v in profile["mask"]]
        valid = profile["valid_indices"]
        print(f"  Sensor profile: '{sensor_profile}' — {profile['description']}")
        print(f"  Active indices: {valid}")

        self.sensor_profile = sensor_profile
        self.valid_indices  = valid

        self.indices_layer = IndicesLayer(
            red_idx=red_idx, nir_idx=nir_idx,
            rededge_idx=rededge_idx,
            sensor_mask=sensor_mask,
        )
        self.cnn = FireRiskCNN(in_channels=3, base_ch=base_ch)

    def forward(self, x):
        return self.cnn(self.indices_layer(x))

    def predict(self, x):
        self.eval()
        with torch.no_grad():
            return self.forward(x)

    def get_indices(self, x):
        self.eval()
        with torch.no_grad():
            return self.indices_layer(x)

    def save(self, path):
        torch.save(self.state_dict(), path)
        print(f"Weights saved -> {path}")

    def load(self, path, device="cpu"):
        self.load_state_dict(torch.load(path, map_location=device))
        print(f"Weights loaded <- {path}")

    def export_onnx(self, path, n_bands, hw=256):
        self.eval()
        dummy = torch.zeros(1, n_bands, hw, hw)
        torch.onnx.export(
            self, dummy, path,
            input_names=["bands"], output_names=["risk_map"],
            opset_version=17,
            dynamic_axes={"bands":    {0: "batch", 2: "H", 3: "W"},
                          "risk_map": {0: "batch", 2: "H", 3: "W"}},
        )
        print(f"ONNX exported -> {path}")


# ===========================================================================
# 4. Training helpers
# ===========================================================================

class FireRiskLoss(nn.Module):
    """Combined BCE + Dice loss for imbalanced fire-scar segmentation."""
    def __init__(self, bce_weight=0.5, dice_weight=0.5):
        super().__init__()
        self.bce_w  = bce_weight
        self.dice_w = dice_weight
        self.bce    = nn.BCELoss()

    def _dice(self, pred, target, smooth=1.0):
        pred_flat   = pred.view(-1)
        target_flat = target.view(-1)
        intersection = (pred_flat * target_flat).sum()
        return 1.0 - (2.0 * intersection + smooth) / (
            pred_flat.sum() + target_flat.sum() + smooth)

    def forward(self, pred, target):
        return self.bce_w * self.bce(pred, target) + \
               self.dice_w * self._dice(pred, target)


def compute_iou(pred: torch.Tensor, target: torch.Tensor,
                threshold: float = 0.5, smooth: float = 1e-6) -> float:
    pred_bin     = (pred >= threshold).float()
    intersection = (pred_bin * target).sum(dim=(1, 2, 3))
    union        = (pred_bin + target).clamp(0, 1).sum(dim=(1, 2, 3))
    return ((intersection + smooth) / (union + smooth)).mean().item()


def train_one_epoch(model, loader, optimizer, loss_fn, device):
    model.train()
    total_loss = 0.0
    for bands, labels in loader:
        bands  = bands.to(device)
        labels = labels.to(device)
        optimizer.zero_grad()
        loss = loss_fn(model(bands), labels)
        loss.backward()
        optimizer.step()
        total_loss += loss.item()
    return total_loss / len(loader)


def evaluate(model, loader, loss_fn, device,
             iou_threshold: float = 0.5) -> tuple[float, float]:
    model.eval()
    total_loss = 0.0
    total_iou  = 0.0
    with torch.no_grad():
        for bands, labels in loader:
            bands  = bands.to(device)
            labels = labels.to(device)
            risk_map = model(bands)
            total_loss += loss_fn(risk_map, labels).item()
            total_iou  += compute_iou(risk_map, labels, threshold=iou_threshold)
    n = len(loader)
    return total_loss / n, total_iou / n


# ===========================================================================
# 5. GeoTIFF I/O
# ===========================================================================

def _norm_band(band: np.ndarray, nodata) -> np.ndarray:
    if nodata is not None:
        band[band == nodata] = 0.0
    valid = band[band > 0]
    p2, p98 = np.percentile(valid, [2, 98]) if valid.size else (0.0, 1.0)
    return np.clip((band - p2) / (p98 - p2 + 1e-6), 0.0, 1.0)


def load_tif_as_tensor(tif_path, band_indices, device="cpu"):
    try:
        import rasterio
    except ImportError:
        raise ImportError("pip install rasterio")

    bands = []
    with rasterio.open(tif_path) as src:
        meta = src.meta.copy()
        for idx in band_indices:
            bands.append(_norm_band(src.read(idx).astype(np.float32), src.nodata))

    arr    = np.stack(bands, axis=0)
    tensor = torch.from_numpy(arr).float().unsqueeze(0).to(device)
    return tensor, meta, (arr.shape[1], arr.shape[2])


def load_tif_from_bytes(tiff_bytes: bytes, band_indices: list[int], device: str = "cpu"):
    try:
        import rasterio
        from rasterio.io import MemoryFile
    except ImportError as exc:
        raise ImportError("pip install rasterio") from exc

    bands = []
    with MemoryFile(tiff_bytes) as memfile:
        with memfile.open() as src:
            for idx in band_indices:
                band = src.read(idx).astype(np.float32)
                nodata = src.nodata
                if nodata is not None:
                    band[band == nodata] = 0.0
                valid = band[band > 0]
                if valid.size > 0:
                    p2, p98 = np.percentile(valid, [2, 98])
                else:
                    p2, p98 = 0.0, 1.0
                band = np.clip((band - p2) / (p98 - p2 + 1e-6), 0.0, 1.0)
                bands.append(band)
            meta = src.meta.copy()
            h, w = bands[0].shape

    arr = np.stack(bands, axis=0)
    tensor = torch.from_numpy(arr).float().unsqueeze(0).to(device)
    return tensor, meta, (h, w)


def save_risk_tif(risk_map, meta, output_path):
    try:
        import rasterio
    except ImportError:
        raise ImportError("pip install rasterio")
    meta.update({"count": 1, "dtype": "float32", "nodata": -1.0})
    with rasterio.open(output_path, "w", **meta) as dst:
        dst.write(risk_map, 1)
    print(f"Risk map saved -> {output_path}")


# ===========================================================================
# 6. Visualisation
# ===========================================================================

def plot_results(indices_tensor, risk_tensor, filename, nir_band=None,
                 save_path=None, valid_indices=None, buf=None):
    """
    Layout:
        Row 0: NIR | NDVI | NDRE | RRI2
        Row 1: Fire Risk Score map  (full width)
        Row 2: Risk Score histogram (full width)

    buf: optional io.BytesIO — write PNG there instead of disk/screen.
    """
    from matplotlib.gridspec import GridSpec

    indices = indices_tensor[0].cpu().numpy()
    risk    = risk_tensor[0, 0].cpu().numpy()

    fig = plt.figure(figsize=(20, 14))
    fig.suptitle(f"Fire Risk Analysis — {filename}", fontsize=15, fontweight="bold", y=0.98)

    gs_top = GridSpec(1, 4, figure=fig, top=0.90, bottom=0.63, wspace=0.05)
    gs_mid = GridSpec(1, 1, figure=fig, top=0.58, bottom=0.30)
    gs_bot = GridSpec(1, 1, figure=fig, top=0.25, bottom=0.02)

    index_cmaps = [
        ["#8B4513","#FFFF00","#006400"],
        ["#5C3317","#FFF176","#1B5E20"],
        ["#8B2500","#FF6B35","#FFD700","#ADFF2F","#004000"],
    ]
    vmins = [-1, -1, 0]
    vmaxs = [ 1,  1, 1]

    ax_nir = fig.add_subplot(gs_top[0, 0])
    if nir_band is not None:
        ax_nir.imshow(nir_band, cmap="gray", vmin=0, vmax=1)
        ax_nir.set_title("NIR (raw)", fontsize=9, fontweight="bold")
    else:
        ax_nir.set_title("NIR (not provided)", fontsize=9, color="#aaaaaa")
    ax_nir.axis("off")

    for i, (name, cmap_colors, vmin, vmax) in enumerate(
        zip(INDEX_NAMES, index_cmaps, vmins, vmaxs)
    ):
        ax = fig.add_subplot(gs_top[0, i + 1])
        cmap = mcolors.LinearSegmentedColormap.from_list(name, cmap_colors)
        ax.imshow(indices[i], cmap=cmap, vmin=vmin, vmax=vmax)
        is_valid = valid_indices is None or name in valid_indices
        colour   = "black" if is_valid else "#aaaaaa"
        suffix   = "" if is_valid else " (N/A)"
        ax.set_title(name + suffix, fontsize=9, fontweight="bold", color=colour)
        ax.axis("off")
        if not is_valid:
            ax.text(0.5, 0.5, "Not valid\nfor this sensor",
                    transform=ax.transAxes, ha="center", va="center",
                    fontsize=8, color="#888888",
                    bbox=dict(boxstyle="round", fc="white", alpha=0.7))

    risk_cmap = mcolors.LinearSegmentedColormap.from_list(
        "fire_risk", ["#2ecc71", "#f39c12", "#e74c3c"]
    )
    ax_risk = fig.add_subplot(gs_mid[0, 0])
    im = ax_risk.imshow(risk, cmap=risk_cmap, vmin=0, vmax=1)
    ax_risk.set_title("Fire Risk Score (0 = safe  →  1 = high risk)", fontsize=11)
    ax_risk.axis("off")
    plt.colorbar(im, ax=ax_risk, fraction=0.02, pad=0.01, label="Risk Score")

    ax_hist = fig.add_subplot(gs_bot[0, 0])
    ax_hist.hist(risk.flatten(), bins=100, color="#e74c3c", edgecolor="none", alpha=0.8)
    ax_hist.axvline(0.3, color="orange", linestyle="--", label="Medium threshold (0.3)")
    ax_hist.axvline(0.6, color="red",    linestyle="--", label="High threshold (0.6)")
    ax_hist.set_title("Risk Score Distribution")
    ax_hist.set_xlabel("Risk Score")
    ax_hist.set_ylabel("Pixel Count")
    ax_hist.legend(fontsize=9)

    total  = risk.size
    low    = np.sum(risk < 0.3)
    medium = np.sum((risk >= 0.3) & (risk < 0.6))
    high   = np.sum(risk >= 0.6)
    print(f"\n  Risk Zone Summary:")
    print(f"  Low    (< 0.3):   {low:>8,} px  ({low/total*100:5.1f}%)")
    print(f"  Medium (0.3-0.6): {medium:>8,} px  ({medium/total*100:5.1f}%)")
    print(f"  High   (> 0.6):   {high:>8,} px  ({high/total*100:5.1f}%)\n")

    if buf is not None:
        plt.savefig(buf, dpi=150, bbox_inches="tight", format="png")
    elif save_path:
        plt.savefig(save_path, dpi=150, bbox_inches="tight")
        print(f"Plot saved -> {save_path}")
    else:
        plt.show()
    plt.close(fig)


# ===========================================================================
# 7. Masking — clouds and non-vegetation
# ===========================================================================

def mask_clouds_from_bands(
    red: np.ndarray,
    nir: np.ndarray,
    brightness_threshold: float = 0.8,
    ndvi_threshold:       float = 0.05,
) -> np.ndarray:
    from scipy.ndimage import binary_dilation
    ndvi       = (nir - red) / (nir + red + 1e-6)
    brightness = (red + nir) / 2.0
    is_cloud = binary_dilation(
        (brightness > brightness_threshold) & (ndvi < ndvi_threshold),
        iterations=5,
    )
    cloud_mask = (~is_cloud).astype(np.float32)
    print(f"  Cloud coverage:      {(cloud_mask < 0.5).mean()*100:.1f}% of image masked")
    return cloud_mask


def mask_non_vegetation(
    red:            np.ndarray,
    nir:            np.ndarray,
    ndvi_threshold: float = 0.2,
    min_nir:        float = 0.1,
) -> np.ndarray:
    ndvi     = (nir - red) / (nir + red + 1e-6)
    veg_mask = ((ndvi > ndvi_threshold) & (nir > min_nir)).astype(np.float32)
    veg_pct  = veg_mask.mean() * 100
    print(f"  Vegetation coverage: {veg_pct:.1f}%  ({100 - veg_pct:.1f}% masked as non-veg)")
    return veg_mask


def apply_masks(
    risk_map:           np.ndarray,
    red:                np.ndarray,
    nir:                np.ndarray,
    ndvi_veg_threshold: float = 0.2,
    cloud_brightness:   float = 0.8,
) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    print("\n  Applying masks...")
    cloud_mask  = mask_clouds_from_bands(red, nir, brightness_threshold=cloud_brightness)
    veg_mask    = mask_non_vegetation(red, nir, ndvi_threshold=ndvi_veg_threshold)
    combined    = cloud_mask * veg_mask
    masked_risk = risk_map * combined
    print(f"  Total masked:        {(combined < 0.5).mean()*100:.1f}% of image")
    return masked_risk, veg_mask, cloud_mask


def inspect_thresholds(tif_path: str, red_band: int = 1, nir_band: int = 2) -> None:
    import rasterio
    with rasterio.open(tif_path) as src:
        red_raw = src.read(red_band).astype(np.float32)
        nir_raw = src.read(nir_band).astype(np.float32)

    def norm(arr):
        valid = arr[arr > 0]
        p2, p98 = np.percentile(valid, [2, 98]) if valid.size else (0, 1)
        return np.clip((arr - p2) / (p98 - p2 + 1e-6), 0.0, 1.0)

    red    = norm(red_raw)
    nir    = norm(nir_raw)
    ndvi   = (nir - red) / (nir + red + 1e-6)
    bright = (red + nir) / 2.0
    print("\n── Band statistics ────────────────────────────────────")
    print(f"  Red    mean={red.mean():.3f}  std={red.std():.3f}  "
          f"p5={np.percentile(red, 5):.3f}  p95={np.percentile(red, 95):.3f}")
    print(f"  NIR    mean={nir.mean():.3f}  std={nir.std():.3f}  "
          f"p5={np.percentile(nir, 5):.3f}  p95={np.percentile(nir, 95):.3f}")
    print(f"  NDVI   mean={ndvi.mean():.3f}  std={ndvi.std():.3f}  "
          f"p5={np.percentile(ndvi, 5):.3f}  p95={np.percentile(ndvi, 95):.3f}")
    print(f"  Bright mean={bright.mean():.3f}  "
          f"p95={np.percentile(bright, 95):.3f}  max={bright.max():.3f}")
    print("\n── Suggested thresholds ───────────────────────────────")
    print(f"  --ndvi-threshold   {np.percentile(ndvi, 20):.2f}")
    print(f"  --cloud-brightness {np.percentile(bright, 97):.2f}")
    print()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Fire-risk CNN: TIFF pipeline consumer and/or file-based inference.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )

    # --- mode ---
    mode = parser.add_argument_group("Mode")
    mode.add_argument("--pipeline",  action="store_true", help="RabbitMQ consumer mode.")
    mode.add_argument("--train",     action="store_true", help="Run training loop.")
    mode.add_argument("--skip-data", action="store_true", help="Skip earth_data download (use cached dataset).")

    # --- file mode ---
    fm = parser.add_argument_group("File mode")
    fm.add_argument("input", nargs="*", metavar="FILE",  help="Input GeoTIFF file(s) to process offline.")
    fm.add_argument("--save-risk",     action="store_true",  help="Save risk GeoTIFF alongside each input.")
    fm.add_argument("--plot",          action="store_true",  help="Show plots interactively (file mode).")
    fm.add_argument("--plot-save",     action="store_true",  help="Save PNG plot alongside each input.")
    fm.add_argument("--plot-save-dir", default=None,         help="Override directory for saved plots.")
    fm.add_argument("--export-onnx",   default=None,         help="Export model to ONNX at this path.")

    # --- pipeline ---
    pl = parser.add_argument_group("Pipeline (--pipeline)")
    pl.add_argument("--amqp-url",       default=PIPELINE_DEFAULTS["url"],
                    help="AMQP URL.")
    pl.add_argument("--tiff-exchange",  default=PIPELINE_DEFAULTS["tiff_exchange"],
                    help="Fanout exchange to consume TIFFs from.")
    pl.add_argument("--out-exchange",   default=PIPELINE_DEFAULTS["out_exchange"],
                    help="Fanout exchange to publish JSON + PNG results to.")
    pl.add_argument("-w", "--write", action="store_true",
                    help="Write per-message JSON and PNG to disk.")
    pl.add_argument("--write-dir", default="pipeline-dumps/model_data_queue",
                    help="Output directory for --write.")

    # --- model ---
    md = parser.add_argument_group("Model")
    md.add_argument("--weights",        default=None,   help="Path to .pt weights file.")
    md.add_argument("--device",         default="cpu",  help="Torch device (cpu / cuda).")
    md.add_argument("--sensor-profile", default="full",
                    choices=list(SENSOR_PROFILES.keys()), help="Spectral index mask profile.")
    md.add_argument("--red",     type=int, default=1,    help="1-based Red band index.")
    md.add_argument("--nir",     type=int, default=2,    help="1-based NIR band index.")
    md.add_argument("--rededge", type=int, default=None, help="1-based RedEdge band index (optional).")

    # --- masking ---
    mk = parser.add_argument_group("Masking")
    mk.add_argument("--mask",             action="store_true", help="Apply cloud + vegetation masking.")
    mk.add_argument("--ndvi-threshold",   type=float, default=0.2,
                    help="NDVI threshold for vegetation masking.")
    mk.add_argument("--cloud-brightness", type=float, default=0.8,
                    help="Mean (R+NIR)/2 threshold for cloud masking.")
    mk.add_argument("--inspect-thresholds", action="store_true",
                    help="Print suggested masking thresholds for the first input file and exit.")

    # --- logging ---
    parser.add_argument("--log", default=None,
                        help="Tee stdout+stderr to this file.")

    return parser.parse_args()


def run_pipeline(args: argparse.Namespace) -> int:
    """RabbitMQ consumer: read TIFFs, infer fire risk, publish JSON + PNG."""
    ordered = ["red", "nir", "rededge"]
    band_map = {"red": args.red, "nir": args.nir, "rededge": args.rededge}
    active          = [b for b in ordered if band_map.get(b)]
    band_indices    = [band_map[b] for b in active]
    ch_idx          = {name: i for i, name in enumerate(active)}

    print("\nBuilding model...")
    model = FireRiskModel(
        red_idx      = ch_idx["red"],
        nir_idx      = ch_idx["nir"],
        rededge_idx  = ch_idx.get("rededge"),
        sensor_profile = args.sensor_profile,
    ).to(args.device)
    if args.weights:
        model.load(args.weights, args.device)

    params = pika.URLParameters(args.amqp_url)
    params.connection_attempts = 5
    params.retry_delay         = 3
    params.socket_timeout      = 10
    params.heartbeat           = 60

    connection = pika.BlockingConnection(params)
    amqp_ch = connection.channel()

    amqp_ch.exchange_declare(exchange=args.tiff_exchange,  exchange_type="fanout", durable=True)
    amqp_ch.exchange_declare(exchange=args.out_exchange,   exchange_type="fanout", durable=True)

    consumer_tag = "fire-risk-model"
    queue_name   = f"{args.tiff_exchange}.{consumer_tag}"
    amqp_ch.queue_declare(queue=queue_name, durable=True, exclusive=False, auto_delete=False)
    amqp_ch.queue_bind(queue=queue_name, exchange=args.tiff_exchange, routing_key="")
    amqp_ch.basic_qos(prefetch_count=1)

    print(f"Connected:  {args.amqp_url}")
    print(f"Consuming:  exchange '{args.tiff_exchange}'  via queue '{queue_name}'")
    print(f"Publishing: JSON + PNG  ->  exchange '{args.out_exchange}'\n")

    write_base = Path(args.write_dir) if args.write else None
    if write_base:
        write_base.mkdir(parents=True, exist_ok=True)

    def on_tiff_message(mq_ch, method, properties, body):
        timestamp = dt.datetime.now(dt.timezone.utc).isoformat()
        msg_id    = getattr(properties, "message_id", None) or uuid.uuid4().hex[:8]
        try:
            tensor, _meta, (h, w) = load_tif_from_bytes(body, band_indices, args.device)
            indices_t = model.get_indices(tensor).cpu()
            risk_t    = model.predict(tensor).cpu()
            risk_np   = risk_t[0, 0].numpy()

            if args.mask:
                red_np = indices_t[0, ch_idx["red"]].numpy()
                nir_np = indices_t[0, ch_idx["nir"]].numpy()
                risk_np, _, _ = apply_masks(
                    risk_np, red_np, nir_np,
                    ndvi_veg_threshold = args.ndvi_threshold,
                    cloud_brightness   = args.cloud_brightness,
                )

            total  = risk_np.size
            low    = int(np.sum(risk_np < FireRiskModel.LOW_THRESHOLD))
            medium = int(np.sum((risk_np >= FireRiskModel.LOW_THRESHOLD) &
                                (risk_np <  FireRiskModel.MEDIUM_THRESHOLD)))
            high   = int(np.sum(risk_np >= FireRiskModel.MEDIUM_THRESHOLD))

            result = {
                "ts":              timestamp,
                "message_id":      msg_id,
                "source_exchange": args.tiff_exchange,
                "image_w":         w,
                "image_h":         h,
                "risk_min":        round(float(risk_np.min()),  4),
                "risk_max":        round(float(risk_np.max()),  4),
                "risk_mean":       round(float(risk_np.mean()), 4),
                "low_pct":         round(low    / total * 100,  2),
                "medium_pct":      round(medium / total * 100,  2),
                "high_pct":        round(high   / total * 100,  2),
            }

            # 1 — Publish JSON stats
            json_bytes = json.dumps(result, ensure_ascii=True).encode("utf-8")
            amqp_ch.basic_publish(
                exchange    = args.out_exchange,
                routing_key = "",
                body        = json_bytes,
                properties  = pika.BasicProperties(
                    content_type = "application/json",
                    delivery_mode = 2,
                    message_id   = msg_id,
                ),
            )

            # 2 — Publish PNG plot
            nir_np   = tensor[0, ch_idx["nir"]].cpu().numpy() if "nir" in ch_idx else None
            filename = getattr(properties, "headers", {}) or {}
            filename = filename.get("filename", msg_id)
            png_buf  = io.BytesIO()
            plot_results(
                indices_t, risk_t, filename,
                nir_band      = nir_np,
                valid_indices = model.valid_indices,
                buf           = png_buf,
            )
            png_bytes = png_buf.getvalue()
            amqp_ch.basic_publish(
                exchange    = args.out_exchange,
                routing_key = "",
                body        = png_bytes,
                properties  = pika.BasicProperties(
                    content_type  = "image/png",
                    delivery_mode = 2,
                    message_id    = msg_id,
                    headers       = {
                        "image_w":   w,
                        "image_h":   h,
                        "risk_mean": result["risk_mean"],
                        "high_pct":  result["high_pct"],
                    },
                ),
            )

            if write_base:
                (write_base / f"{msg_id}.json").write_text(
                    json.dumps(result, indent=2), encoding="utf-8")
                (write_base / f"{msg_id}.png").write_bytes(png_bytes)

            print(
                f"[{timestamp}] id={msg_id}  in={len(body):,}B  "
                f"json={len(json_bytes)}B  png={len(png_bytes):,}B  "
                f"mean={result['risk_mean']:.3f}  high={result['high_pct']}%"
            )
            mq_ch.basic_ack(delivery_tag=method.delivery_tag)

        except Exception as exc:
            print(f"[{timestamp}] id={msg_id}  error={exc}")
            mq_ch.basic_nack(delivery_tag=method.delivery_tag, requeue=True)

    amqp_ch.basic_consume(queue=queue_name, on_message_callback=on_tiff_message, auto_ack=False)

    try:
        amqp_ch.start_consuming()
    except KeyboardInterrupt:
        pass
    finally:
        if amqp_ch.is_open:   amqp_ch.close()
        if connection.is_open: connection.close()

    return 0


# ===========================================================================
# 9. Logging helper
# ===========================================================================

class _Tee:
    """Duplicate sys.stdout / sys.stderr to a log file."""
    def __init__(self, path):
        import sys
        self._log  = open(path, "a", encoding="utf-8")
        self._stdout = sys.stdout
        self._stderr = sys.stderr
        sys.stdout = self
        sys.stderr = self

    def write(self, msg):
        self._stdout.write(msg)
        self._log.write(msg)

    def flush(self):
        self._stdout.flush()
        self._log.flush()

    def close(self):
        import sys
        sys.stdout = self._stdout
        sys.stderr = self._stderr
        self._log.close()


# ===========================================================================
# 10. Entry point
# ===========================================================================

def main() -> int:
    args = parse_args()

    tee = _Tee(args.log) if args.log else None
    try:
        return _run(args)
    finally:
        if tee:
            tee.close()


def _run(args) -> int:  # noqa: C901
    # --- training mode ---
    if args.train:
        try:
            import earth_data  # type: ignore
        except ImportError:
            print("Training requires the 'earth_data' package."
                  "  Install it or use --skip-data.")
            return 1

        device = torch.device(args.device)
        print("\nBuilding model for training...")
        model = FireRiskModel(
            red_idx        = args.red - 1,
            nir_idx        = args.nir - 1,
            rededge_idx    = (args.rededge - 1) if args.rededge else None,
            sensor_profile = args.sensor_profile,
        ).to(device)
        if args.weights:
            model.load(args.weights, device)

        loaders = earth_data.get_loaders(skip_download=args.skip_data)
        loss_fn   = FireRiskLoss()
        optimizer = torch.optim.Adam(model.parameters(), lr=1e-4)

        best_iou = 0.0
        for epoch in range(1, 51):
            t_loss = train_one_epoch(model, loaders["train"], optimizer, loss_fn, device)
            v_loss, v_iou = evaluate(model, loaders["val"], loss_fn, device)
            print(f"Epoch {epoch:03d}  train={t_loss:.4f}  val={v_loss:.4f}  IoU={v_iou:.3f}")
            if v_iou > best_iou:
                best_iou = v_iou
                save_path = args.weights or "fire_risk_best.pt"
                model.save(save_path)

        if args.export_onnx:
            n_bands = args.red  # approximate; adjust if needed
            model.export_onnx(args.export_onnx, n_bands=n_bands)
        return 0

    # --- pipeline mode ---
    if args.pipeline:
        return run_pipeline(args)

    # --- file mode ---
    if not args.input:
        print("Provide input GeoTIFF files, --pipeline, or --train.")
        return 2

    if args.inspect_thresholds:
        inspect_thresholds(args.input[0], red_band=args.red, nir_band=args.nir)
        return 0

    ordered = ["red", "nir", "rededge"]
    band_map = {"red": args.red, "nir": args.nir, "rededge": args.rededge}
    active       = [b for b in ordered if band_map.get(b)]
    band_indices = [band_map[b] for b in active]
    ch_idx       = {name: i for i, name in enumerate(active)}

    device = torch.device(args.device)
    print("\nBuilding model...")
    model = FireRiskModel(
        red_idx        = ch_idx["red"],
        nir_idx        = ch_idx["nir"],
        rededge_idx    = ch_idx.get("rededge"),
        sensor_profile = args.sensor_profile,
    ).to(device)
    if args.weights:
        model.load(args.weights, str(device))

    if args.export_onnx:
        model.export_onnx(args.export_onnx, n_bands=max(band_indices))

    for tif_path in args.input:
        print(f"\nProcessing: {tif_path}")
        tensor, meta, (h, w) = load_tif_as_tensor(tif_path, band_indices, str(device))
        indices_t = model.get_indices(tensor).cpu()
        risk_t    = model.predict(tensor).cpu()
        risk_np   = risk_t[0, 0].numpy()

        if args.mask:
            red_np = tensor[0, ch_idx["red"]].cpu().numpy()
            nir_np = tensor[0, ch_idx["nir"]].cpu().numpy()
            risk_np, _, _ = apply_masks(
                risk_np, red_np, nir_np,
                ndvi_veg_threshold = args.ndvi_threshold,
                cloud_brightness   = args.cloud_brightness,
            )

        if args.save_risk:
            out_path = Path(tif_path).with_suffix(".risk.tif")
            save_risk_tif(risk_np, meta, str(out_path))

        nir_np    = tensor[0, ch_idx["nir"]].cpu().numpy() if "nir" in ch_idx else None
        save_path = None
        if args.plot_save:
            d = Path(args.plot_save_dir) if args.plot_save_dir else Path(tif_path).parent
            d.mkdir(parents=True, exist_ok=True)
            save_path = str(d / (Path(tif_path).stem + "_fire_risk.png"))

        plot_results(
            indices_t, risk_t, Path(tif_path).name,
            nir_band      = nir_np,
            save_path     = save_path,
            valid_indices = model.valid_indices,
        )
        if args.plot and not args.plot_save:
            plt.show()

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
