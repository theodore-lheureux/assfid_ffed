#include <cuda_runtime.h>
#include <stdint.h>

#define PIX(in, x, y, w) in[(x) + (y) * (w)]

// Inline normalization helper - avoid lambda overhead
__device__ __forceinline__ float normalize_pixel(uint16_t val, int black, float inv_range) {
    int v = (int)val - black;
    return fmaxf((float)v * inv_range, 0.0f);
}

// Helper to get red channel at any pixel
__device__ __forceinline__ float get_red(const uint16_t* __restrict__ in, int x, int y, int w, int black, float inv_range) {
    // Use bitwise AND instead of modulo for power-of-2 operations
    int x_odd = x & 1;
    int y_odd = y & 1;
    
    // RGGB: R at (even, even)
    if (!x_odd && !y_odd) {
        // Red pixel - return directly
        return normalize_pixel(PIX(in, x, y, w), black, inv_range);
    } else if (x_odd && !y_odd) {
        // Green on R row - interpolate horizontally
        return (normalize_pixel(PIX(in, x-1, y, w), black, inv_range) + 
                normalize_pixel(PIX(in, x+1, y, w), black, inv_range)) * 0.5f;
    } else if (!x_odd && y_odd) {
        // Green on B row - interpolate vertically
        return (normalize_pixel(PIX(in, x, y-1, w), black, inv_range) + 
                normalize_pixel(PIX(in, x, y+1, w), black, inv_range)) * 0.5f;
    } else {
        // Blue pixel - interpolate diagonally
        return (normalize_pixel(PIX(in, x-1, y-1, w), black, inv_range) + 
                normalize_pixel(PIX(in, x+1, y-1, w), black, inv_range) +
                normalize_pixel(PIX(in, x-1, y+1, w), black, inv_range) + 
                normalize_pixel(PIX(in, x+1, y+1, w), black, inv_range)) * 0.25f;
    }
}

// Helper to get green channel at any pixel
__device__ __forceinline__ float get_green(const uint16_t* __restrict__ in, int x, int y, int w, int black, float inv_range) {
    // RGGB: G at (odd, even) and (even, odd)
    if ((x + y) & 1) {
        // Green pixel - return directly
        return normalize_pixel(PIX(in, x, y, w), black, inv_range);
    } else {
        // Red or Blue pixel - interpolate from 4 neighbors
        return (normalize_pixel(PIX(in, x-1, y, w), black, inv_range) + 
                normalize_pixel(PIX(in, x+1, y, w), black, inv_range) +
                normalize_pixel(PIX(in, x, y-1, w), black, inv_range) + 
                normalize_pixel(PIX(in, x, y+1, w), black, inv_range)) * 0.25f;
    }
}

// Helper to get blue channel at any pixel
__device__ __forceinline__ float get_blue(const uint16_t* __restrict__ in, int x, int y, int w, int black, float inv_range) {
    int x_odd = x & 1;
    int y_odd = y & 1;
    
    // RGGB: B at (odd, odd)
    if (x_odd && y_odd) {
        // Blue pixel - return directly
        return normalize_pixel(PIX(in, x, y, w), black, inv_range);
    } else if (!x_odd && y_odd) {
        // Green on B row - interpolate horizontally
        return (normalize_pixel(PIX(in, x-1, y, w), black, inv_range) + 
                normalize_pixel(PIX(in, x+1, y, w), black, inv_range)) * 0.5f;
    } else if (x_odd && !y_odd) {
        // Green on R row - interpolate vertically
        return (normalize_pixel(PIX(in, x, y-1, w), black, inv_range) + 
                normalize_pixel(PIX(in, x, y+1, w), black, inv_range)) * 0.5f;
    } else {
        // Red pixel - interpolate diagonally
        return (normalize_pixel(PIX(in, x-1, y-1, w), black, inv_range) + 
                normalize_pixel(PIX(in, x+1, y-1, w), black, inv_range) +
                normalize_pixel(PIX(in, x-1, y+1, w), black, inv_range) + 
                normalize_pixel(PIX(in, x+1, y+1, w), black, inv_range)) * 0.25f;
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

    // Pre-compute inverse range (multiply is faster than divide)
    float inv_range = 1.0f / (float)(white_level - black_level);
    
    // Get camera RGB values with white balance applied
    float cam_r = get_red(in, x, y, width, black_level, inv_range) * wb_r;
    float cam_g = get_green(in, x, y, width, black_level, inv_range) * wb_g;
    float cam_b = get_blue(in, x, y, width, black_level, inv_range) * wb_b;
    
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
    int idx = (y * width + x) * 3;
    out[idx]     = fminf(fmaxf(srgb_r, 0.0f), 1.0f);
    out[idx + 1] = fminf(fmaxf(srgb_g, 0.0f), 1.0f);
    out[idx + 2] = fminf(fmaxf(srgb_b, 0.0f), 1.0f);
}