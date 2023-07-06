#include <stdint.h>

#include "../alvr/server/cpp/alvr_server/bindings.h"

const unsigned char *FRAME_RENDER_VS_CSO_PTR;
unsigned int FRAME_RENDER_VS_CSO_LEN;
const unsigned char *FRAME_RENDER_PS_CSO_PTR;
unsigned int FRAME_RENDER_PS_CSO_LEN;
const unsigned char *QUAD_SHADER_CSO_PTR;
unsigned int QUAD_SHADER_CSO_LEN;
const unsigned char *COMPRESS_AXIS_ALIGNED_CSO_PTR;
unsigned int COMPRESS_AXIS_ALIGNED_CSO_LEN;
const unsigned char *COLOR_CORRECTION_CSO_PTR;
unsigned int COLOR_CORRECTION_CSO_LEN;

const unsigned char *QUAD_SHADER_COMP_SPV_PTR;
unsigned int QUAD_SHADER_COMP_SPV_LEN;
const unsigned char *COLOR_SHADER_COMP_SPV_PTR;
unsigned int COLOR_SHADER_COMP_SPV_LEN;
const unsigned char *FFR_SHADER_COMP_SPV_PTR;
unsigned int FFR_SHADER_COMP_SPV_LEN;
const unsigned char *RGBTOYUV420_SHADER_COMP_SPV_PTR;
unsigned int RGBTOYUV420_SHADER_COMP_SPV_LEN;

const char *g_sessionPath;
const char *g_driverRootDir;

void (*LogError)(const char *stringPtr);
void (*LogWarn)(const char *stringPtr);
void (*LogInfo)(const char *stringPtr);
void (*LogDebug)(const char *stringPtr);
void (*LogPeriodically)(const char *tag, const char *stringPtr);
void (*DriverReadyIdle)(bool setDefaultChaprone);
void (*InitializeDecoder)(const unsigned char *configBuffer, int len, int codec);
void (*VideoSend)(unsigned long long targetTimestampNs,
                             unsigned char *buf,
                             int len,
                             bool isIdr);
void (*HapticsSend)(unsigned long long path,
                               float duration_s,
                               float frequency,
                               float amplitude);
void (*ShutdownRuntime)();
unsigned long long (*PathStringToHash)(const char *path);
void (*ReportPresent)(unsigned long long timestamp_ns, unsigned long long offset_ns);
void (*ReportComposed)(unsigned long long timestamp_ns, unsigned long long offset_ns);
FfiDynamicEncoderParams (*GetDynamicEncoderParams)();
unsigned long long (*GetSerialNumber)(unsigned long long deviceID, char *outString);
void (*SetOpenvrProps)(unsigned long long deviceID);
void (*WaitForVSync)();

void *CppEntryPoint(const char *pInterfaceName, int *pReturnCode) {
  // Callback from HmdDriverFactory
  *pReturnCode = 0;
  return nullptr;
}
void InitializeStreaming() {
}
void DeinitializeStreaming() {
}
void SendVSync() {
}
void RequestIDR() {
}
void SetTracking(unsigned long long targetTimestampNs,
                            float controllerPoseTimeOffsetS,
                            const FfiDeviceMotion *deviceMotions,
                            int motionsCount,
                            const FfiHandSkeleton *leftHand,
                            const FfiHandSkeleton *rightHand,
                            unsigned int controllersTracked) {
}
void VideoErrorReportReceive() {
}
void ShutdownSteamvr() {
}

void SetOpenvrProperty(unsigned long long deviceID, FfiOpenvrProperty prop) {
}
void SetChaperone(float areaWidth, float areaHeight) {
}
void SetViewsConfig(FfiViewsConfig config) {
}
void SetBattery(unsigned long long deviceID, float gauge_value, bool is_plugged) {
}
void SetButton(unsigned long long path, FfiButtonValue value) {
}

void CaptureFrame() {
}

// NalParsing.cpp
void ParseFrameNals(
    int codec, unsigned char *buf, int len, unsigned long long targetTimestampNs, bool isIdr) {
}

extern "C" {
void* HmdDriverFactory(const char* interface_name, int32_t* return_code);
void CFRunLoopRun(void);
}

int main() {
  int32_t ret;
  HmdDriverFactory("hello", &ret);
  DriverReadyIdle(false);
  CFRunLoopRun();
}
