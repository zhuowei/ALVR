#include "EncodePipelineSW.h"

#include <chrono>

namespace alvr {

namespace
{

void x264_log(void *, int level, const char *fmt, va_list args)
{
    char buf[256];
    vsnprintf(buf, sizeof(buf), fmt, args);
    printf("x264: %s\n", buf);
}

}

EncodePipelineSW::EncodePipelineSW(uint32_t width, uint32_t height)
{
  x264_param_default_preset(&param, "ultrafast", "zerolatency");
  x264_param_apply_profile(&param, "high");

  param.pf_log = x264_log;
  param.i_log_level = X264_LOG_INFO;

  param.b_aud = 0;
  param.b_cabac = false; // settings.m_entropyCoding == ALVR_CABAC;
  param.b_sliced_threads = true;
  param.i_threads = X264_THREADS_AUTO; // settings.m_swThreadCount;
  param.i_width = width;
  param.i_height = height;
  param.rc.i_rc_method = X264_RC_ABR;

  auto params = FfiDynamicEncoderParams {};
  params.updated = true;
  params.bitrate_bps = 30'000'000;
  params.framerate = refresh_rate_; // Settings::Instance().m_refreshRate;
  SetParams(params);

  enc = x264_encoder_open(&param);
  if (!enc) {
    throw std::runtime_error("Failed to open encoder");
  }

  x264_picture_init(&picture);
  picture.img.i_csp = X264_CSP_I420;
  picture.img.i_plane = 3;

  x264_picture_init(&picture_out);
}

EncodePipelineSW::~EncodePipelineSW() {
  if (enc) {
    x264_encoder_close(enc);
  }
}

void EncodePipelineSW::PushFrame(uint64_t targetTimestampNs, bool idr)
{
  timestamp.cpu = std::chrono::duration_cast<std::chrono::nanoseconds>(std::chrono::steady_clock::now().time_since_epoch()).count();

  picture.i_type = idr ? X264_TYPE_IDR : X264_TYPE_AUTO;
  pts = picture.i_pts = targetTimestampNs;

  int nnal = 0;
  nal_size = x264_encoder_encode(enc, &nal, &nnal, &picture, &picture_out);
  if (nal_size < 0) {
    throw std::runtime_error("x264 encoder_encode failed");
  }
}

#if 0
bool EncodePipelineSW::GetEncoded(FramePacket &packet)
{
  if (!nal) {
    return false;
  }
  packet.size = nal_size;
  packet.data = nal[0].p_payload;
  packet.pts = pts;
  return packet.size > 0;
}
#endif

void EncodePipelineSW::SetParams(FfiDynamicEncoderParams params)
{
  if (!params.updated) {
    return;
  }
  // x264 doesn't work well with adaptive bitrate/fps
  param.i_fps_num = refresh_rate_;
  param.i_fps_den = 1;
  param.rc.i_bitrate = params.bitrate_bps / 1'000 * 1.4; // needs higher value to hit target bitrate
  param.rc.i_vbv_buffer_size = param.rc.i_bitrate / param.i_fps_num * 1.1;
  param.rc.i_vbv_max_bitrate = param.rc.i_bitrate;
  param.rc.f_vbv_buffer_init = 0.75;
  if (enc) {
    x264_encoder_reconfig(enc, &param);
  }
}

} // namespace alvr
