// Thin extern "C" surface over the vendored WORLD C++ API, flattening
// the double** spectrogram arguments so Rust passes contiguous
// buffers. All allocation is caller-side.
#include "world/harvest.h"
#include "world/cheaptrick.h"
#include "world/d4c.h"
#include "world/synthesis.h"
#include <vector>
#include <cstddef>
using std::size_t;

extern "C" {

int fts_world_num_frames(int x_length, int fs, double frame_period_ms) {
    HarvestOption option;
    InitializeHarvestOption(&option);
    option.frame_period = frame_period_ms;
    return GetSamplesForHarvest(fs, x_length, option.frame_period);
}

void fts_world_harvest(const double* x, int x_length, int fs,
                       double frame_period_ms, double f0_floor,
                       double f0_ceil, double* temporal_positions,
                       double* f0) {
    HarvestOption option;
    InitializeHarvestOption(&option);
    option.frame_period = frame_period_ms;
    if (f0_floor > 0.0) option.f0_floor = f0_floor;
    if (f0_ceil > 0.0) option.f0_ceil = f0_ceil;
    Harvest(x, x_length, fs, &option, temporal_positions, f0);
}

int fts_world_fft_size(int fs) {
    CheapTrickOption option;
    InitializeCheapTrickOption(fs, &option);
    return option.fft_size;
}

// sp is a flat [f0_length][fft_size/2+1] buffer.
void fts_world_cheaptrick(const double* x, int x_length, int fs,
                          const double* temporal_positions,
                          const double* f0, int f0_length, double* sp) {
    CheapTrickOption option;
    InitializeCheapTrickOption(fs, &option);
    int bins = option.fft_size / 2 + 1;
    std::vector<double*> rows(f0_length);
    for (int i = 0; i < f0_length; ++i) rows[i] = sp + (size_t)i * bins;
    CheapTrick(x, x_length, fs, temporal_positions, f0, f0_length,
               &option, rows.data());
}

void fts_world_d4c(const double* x, int x_length, int fs,
                   const double* temporal_positions, const double* f0,
                   int f0_length, int fft_size, double* ap) {
    D4COption option;
    InitializeD4COption(&option);
    int bins = fft_size / 2 + 1;
    std::vector<double*> rows(f0_length);
    for (int i = 0; i < f0_length; ++i) rows[i] = ap + (size_t)i * bins;
    D4C(x, x_length, fs, temporal_positions, f0, f0_length, fft_size,
        &option, rows.data());
}

void fts_world_synthesis(const double* f0, int f0_length,
                         const double* sp, const double* ap,
                         int fft_size, double frame_period_ms, int fs,
                         int y_length, double* y) {
    int bins = fft_size / 2 + 1;
    std::vector<const double*> sp_rows(f0_length);
    std::vector<const double*> ap_rows(f0_length);
    for (int i = 0; i < f0_length; ++i) {
        sp_rows[i] = sp + (size_t)i * bins;
        ap_rows[i] = ap + (size_t)i * bins;
    }
    Synthesis(f0, f0_length,
              const_cast<const double* const*>(sp_rows.data()),
              const_cast<const double* const*>(ap_rows.data()),
              fft_size, frame_period_ms, fs, y_length, y);
}

}  // extern "C"

