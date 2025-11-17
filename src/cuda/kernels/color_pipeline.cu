#include <cuda_runtime.h>
#include <stdint.h>

// Apply white balance, color matrix, and exposure to already-debayered RGB data
extern "C" __global__ void apply_color_pipeline(
    const uint16_t* __restrict__ rgb_in,
    float* __restrict__ rgb_out,
    int width, int height,
    float wb_r, float wb_g, float wb_b,
    int black_level,
    int white_level,
    const float* __restrict__ cam_to_xyz  // 3x4 matrix in row-major order
) {
    int x = blockDim.x * blockIdx.x + threadIdx.x;
    int y = blockDim.y * blockIdx.y + threadIdx.y;

    if (x >= width || y >= height)
        return;

    int idx = (y * width + x) * 3;
    
    // Pre-compute inverse range (multiply is faster than divide)
    float inv_range = 1.0f / (float)(white_level - black_level);
    
    // Read RGB values from NPP debayered output
    uint16_t r_raw = rgb_in[idx];
    uint16_t g_raw = rgb_in[idx + 1];
    uint16_t b_raw = rgb_in[idx + 2];
    
    // Normalize and apply black/white levels
    float r_norm = fmaxf((float)((int)r_raw - black_level) * inv_range, 0.0f);
    float g_norm = fmaxf((float)((int)g_raw - black_level) * inv_range, 0.0f);
    float b_norm = fmaxf((float)((int)b_raw - black_level) * inv_range, 0.0f);
    
    // Apply white balance
    float cam_r = r_norm * wb_r;
    float cam_g = g_norm * wb_g;
    float cam_b = b_norm * wb_b;
    
    // Apply camera-to-XYZ color matrix (3x4 matrix, row-major)
    // Using FMA (fused multiply-add) for better performance and precision
    float xyz_x = __fmaf_rn(cam_to_xyz[0], cam_r, __fmaf_rn(cam_to_xyz[1], cam_g, __fmaf_rn(cam_to_xyz[2], cam_b, cam_to_xyz[3])));
    float xyz_y = __fmaf_rn(cam_to_xyz[4], cam_r, __fmaf_rn(cam_to_xyz[5], cam_g, __fmaf_rn(cam_to_xyz[6], cam_b, cam_to_xyz[7])));
    float xyz_z = __fmaf_rn(cam_to_xyz[8], cam_r, __fmaf_rn(cam_to_xyz[9], cam_g, __fmaf_rn(cam_to_xyz[10], cam_b, cam_to_xyz[11])));
    
    // Convert XYZ to sRGB (D65 illuminant) with FMA operations
    const float exposure = 3.5f;
    
    float srgb_r = __fmaf_rn( 3.2404542f, xyz_x, __fmaf_rn(-1.5371385f, xyz_y, -0.4985314f * xyz_z)) * exposure;
    float srgb_g = __fmaf_rn(-0.9692660f, xyz_x, __fmaf_rn( 1.8760108f, xyz_y,  0.0415560f * xyz_z)) * exposure;
    float srgb_b = __fmaf_rn( 0.0556434f, xyz_x, __fmaf_rn(-0.2040259f, xyz_y,  1.0572252f * xyz_z)) * exposure;
    
    // Clamp and write output
    rgb_out[idx]     = fminf(fmaxf(srgb_r, 0.0f), 1.0f);
    rgb_out[idx + 1] = fminf(fmaxf(srgb_g, 0.0f), 1.0f);
    rgb_out[idx + 2] = fminf(fmaxf(srgb_b, 0.0f), 1.0f);
}
