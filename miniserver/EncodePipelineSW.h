#pragma once

#include <cstdint>
#include <x264.h>
#include "../alvr/server/cpp/alvr_server/bindings.h"

namespace alvr
{

  struct Timestamp {
    uint64_t gpu = 0;
    uint64_t cpu = 0;
  };

class EncodePipelineSW
{
public:
  ~EncodePipelineSW();
  EncodePipelineSW(uint32_t width, uint32_t height);

  void PushFrame(uint64_t targetTimestampNs, bool idr);
  void SetParams(FfiDynamicEncoderParams params);

  Timestamp timestamp;

  x264_t *enc = nullptr;
  x264_param_t param;
  x264_picture_t picture;
  x264_picture_t picture_out;
  x264_nal_t *nal = nullptr;
  int nal_size = 0;
  int64_t pts = 0;
  int refresh_rate_ = 60;
};
}
