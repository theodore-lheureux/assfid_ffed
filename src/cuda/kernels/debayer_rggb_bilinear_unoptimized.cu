#include <cuda_runtime.h>
#include <stdint.h>

#define PIX(in, x, y, w) in[(x) + (y) * (w)]

// Helper to get red channel at any pixel
__device__ float get_red(const uint16_t* in, int x, int y, int w, int black, float range) {
    auto normalize = [black, range](uint16_t val) {
        int v = (int)val - black;
        if (v < 0) v = 0;
        return (float)v / range;
    };
    
    // RGGB: R at (even, even)
    if (x % 2 == 0 && y % 2 == 0) {
        // Red pixel - return directly
        return normalize(PIX(in, x, y, w));
    } else if (x % 2 == 1 && y % 2 == 0) {
        // Green on R row - interpolate horizontally
        return (normalize(PIX(in, x-1, y, w)) + normalize(PIX(in, x+1, y, w))) / 2.0f;
    } else if (x % 2 == 0 && y % 2 == 1) {
        // Green on B row - interpolate vertically
        return (normalize(PIX(in, x, y-1, w)) + normalize(PIX(in, x, y+1, w))) / 2.0f;
    } else {
        // Blue pixel - interpolate diagonally
        return (normalize(PIX(in, x-1, y-1, w)) + normalize(PIX(in, x+1, y-1, w)) +
                normalize(PIX(in, x-1, y+1, w)) + normalize(PIX(in, x+1, y+1, w))) / 4.0f;
    }
}

// Helper to get green channel at any pixel
__device__ float get_green(const uint16_t* in, int x, int y, int w, int black, float range) {
    auto normalize = [black, range](uint16_t val) {
        int v = (int)val - black;
        if (v < 0) v = 0;
        return (float)v / range;
    };
    
    // RGGB: G at (odd, even) and (even, odd)
    if ((x + y) % 2 == 1) {
        // Green pixel - return directly
        return normalize(PIX(in, x, y, w));
    } else {
        // Red or Blue pixel - interpolate from 4 neighbors
        return (normalize(PIX(in, x-1, y, w)) + normalize(PIX(in, x+1, y, w)) +
                normalize(PIX(in, x, y-1, w)) + normalize(PIX(in, x, y+1, w))) / 4.0f;
    }
}

// Helper to get blue channel at any pixel
__device__ float get_blue(const uint16_t* in, int x, int y, int w, int black, float range) {
    auto normalize = [black, range](uint16_t val) {
        int v = (int)val - black;
        if (v < 0) v = 0;
        return (float)v / range;
    };
    
    // RGGB: B at (odd, odd)
    if (x % 2 == 1 && y % 2 == 1) {
        // Blue pixel - return directly
        return normalize(PIX(in, x, y, w));
    } else if (x % 2 == 0 && y % 2 == 1) {
        // Green on B row - interpolate horizontally
        return (normalize(PIX(in, x-1, y, w)) + normalize(PIX(in, x+1, y, w))) / 2.0f;
    } else if (x % 2 == 1 && y % 2 == 0) {
        // Green on R row - interpolate vertically
        return (normalize(PIX(in, x, y-1, w)) + normalize(PIX(in, x, y+1, w))) / 2.0f;
    } else {
        // Red pixel - interpolate diagonally
        return (normalize(PIX(in, x-1, y-1, w)) + normalize(PIX(in, x+1, y-1, w)) +
                normalize(PIX(in, x-1, y+1, w)) + normalize(PIX(in, x+1, y+1, w))) / 4.0f;
    }
}

extern "C" __global__ void debayer16_to_xyz(
    const uint16_t* __restrict__ in,
    float* __restrict__ out,
    int width, int height,
    float wb_r, float wb_g, float wb_b,
    int black_level,
    int white_level,
    const float* __restrict__ cam_to_xyz  // 3x4 matrix in row-major order
) {
    int x = blockDim.x * blockIdx.x + threadIdx.x;
    int y = blockDim.y * blockIdx.y + threadIdx.y;

    // Need 1-pixel border for interpolation
    if (x < 1 || x >= width - 1 || y < 1 || y >= height - 1)
        return;

    float range = (float)(white_level - black_level);
    
    // Get camera RGB values with white balance
    float cam_r = get_red(in, x, y, width, black_level, range) * wb_r;
    float cam_g = get_green(in, x, y, width, black_level, range) * wb_g;
    float cam_b = get_blue(in, x, y, width, black_level, range) * wb_b;
    
    // Apply camera-to-XYZ color matrix (3x4 matrix, row-major)
    float xyz_x = cam_to_xyz[0] * cam_r + cam_to_xyz[1] * cam_g + cam_to_xyz[2] * cam_b + cam_to_xyz[3];
    float xyz_y = cam_to_xyz[4] * cam_r + cam_to_xyz[5] * cam_g + cam_to_xyz[6] * cam_b + cam_to_xyz[7];
    float xyz_z = cam_to_xyz[8] * cam_r + cam_to_xyz[9] * cam_g + cam_to_xyz[10] * cam_b + cam_to_xyz[11];
    
    // Convert XYZ to sRGB (D65 illuminant)
    // Using standard XYZ to sRGB conversion matrix
    float srgb_r =  3.2404542f * xyz_x - 1.5371385f * xyz_y - 0.4985314f * xyz_z;
    float srgb_g = -0.9692660f * xyz_x + 1.8760108f * xyz_y + 0.0415560f * xyz_z;
    float srgb_b =  0.0556434f * xyz_x - 0.2040259f * xyz_y + 1.0572252f * xyz_z;
    
    // Apply exposure boost to compensate for linear data
    float exposure = 3.5f;
    srgb_r *= exposure;
    srgb_g *= exposure;
    srgb_b *= exposure;
    
    // Clamp and write output
    int idx = (y * width + x) * 3;
    out[idx] = fminf(fmaxf(srgb_r, 0.0f), 1.0f);
    out[idx + 1] = fminf(fmaxf(srgb_g, 0.0f), 1.0f);
    out[idx + 2] = fminf(fmaxf(srgb_b, 0.0f), 1.0f);
}
