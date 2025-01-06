#include "wrapper.hpp"
#include <algorithm>
#include <memory>

// Define platform-specific types before WebRTC includes
#if defined(_WIN32)
#define WEBRTC_WIN 
#define NOMINMAX
namespace rtc {
typedef void* PlatformFile;
}
#else
#define WEBRTC_POSIX
namespace rtc {
typedef int PlatformFile;
}
#endif

#include "absl/types/optional.h"

namespace webrtc_audio_processing_wrapper {
namespace {

OptionalDouble from_absl_optional(const absl::optional<double>& optional) {
  OptionalDouble rv;
  rv.has_value = optional.has_value();
  rv.value = optional.value_or(0.0);
  return rv;
}

OptionalInt from_absl_optional(const absl::optional<int>& optional) {
  OptionalInt rv;
  rv.has_value = optional.has_value();
  rv.value = optional.value_or(0);
  return rv;
}

OptionalBool from_absl_optional(const absl::optional<bool>& optional) {
  OptionalBool rv;
  rv.has_value = optional.has_value();
  rv.value = optional.value_or(false);
  return rv;
}

}  // namespace

struct AudioProcessing {
  std::unique_ptr<webrtc::AudioProcessing> processor;
  webrtc::Config config;
  webrtc::StreamConfig capture_stream_config;
  webrtc::StreamConfig render_stream_config;
  absl::optional<int> stream_delay_ms;
};

AudioProcessing* audio_processing_create(
    int num_capture_channels,
    int num_render_channels,
    int sample_rate_hz,
    int* error) {
  AudioProcessing* ap = new AudioProcessing;
  ap->processor.reset(webrtc::AudioProcessing::Create());

  const bool has_keyboard = false;
  ap->capture_stream_config = webrtc::StreamConfig(
      sample_rate_hz, num_capture_channels, has_keyboard);
  ap->render_stream_config = webrtc::StreamConfig(
      sample_rate_hz, num_render_channels, has_keyboard);

  // The input and output streams must have the same number of channels.
  webrtc::ProcessingConfig pconfig = {
    ap->capture_stream_config, // capture input
    ap->capture_stream_config, // capture output
    ap->render_stream_config,  // render input
    ap->render_stream_config,  // render output
  };
  const int code = ap->processor->Initialize(pconfig);
  if (code != webrtc::AudioProcessing::kNoError) {
    *error = code;
    delete ap;
    return nullptr;
  }

  return ap;
}

void initialize(AudioProcessing* ap) {
  ap->processor->Initialize();
}

int process_capture_frame(AudioProcessing* ap, float** channels) {
  if (ap->processor->echo_cancellation()->is_enabled()) {
    ap->processor->set_stream_delay_ms(
        ap->stream_delay_ms.value_or(0));
  }

  return ap->processor->ProcessStream(
      channels, ap->capture_stream_config, ap->capture_stream_config, channels);
}

int process_render_frame(AudioProcessing* ap, float** channels) {
  return ap->processor->ProcessReverseStream(
      channels, ap->render_stream_config, ap->render_stream_config, channels);
}

Stats get_stats(AudioProcessing* ap) {
  auto* level_est = ap->processor->level_estimator();
  auto* voice_det = ap->processor->voice_detection();
  auto* aec = ap->processor->echo_cancellation();
  webrtc::EchoCancellation::Metrics metrics;
  if (aec) {
    aec->GetMetrics(&metrics);
  }

  return Stats {
    from_absl_optional(level_est ? level_est->RMS() : absl::optional<int>()),
    from_absl_optional(voice_det ? voice_det->stream_has_voice() : absl::optional<bool>()),
    from_absl_optional(aec ? absl::optional<double>(static_cast<double>(metrics.echo_return_loss)) : absl::optional<double>()),
    from_absl_optional(aec ? absl::optional<double>(static_cast<double>(metrics.echo_return_loss_enhancement)) : absl::optional<double>()),
    OptionalDouble{},  // divergent_filter_fraction
    OptionalInt{},     // delay_median_ms
    OptionalInt{},     // delay_standard_deviation_ms
    OptionalDouble{},  // residual_echo_likelihood
    OptionalDouble{},  // residual_echo_likelihood_recent_max
    OptionalInt{},     // delay_ms
  };
}

int get_num_samples_per_frame(AudioProcessing* ap) {
    return ap->capture_stream_config.sample_rate_hz() * webrtc::AudioProcessing::kChunkSizeMs / 1000;
}

void set_config(AudioProcessing* ap, const webrtc::ProcessingConfig& config) {
  ap->processor->Initialize(config);
}

void set_runtime_setting(AudioProcessing* ap, webrtc::Config setting) {
  ap->processor->Initialize(webrtc::ProcessingConfig());  // Reset with default config
}

void set_stream_delay_ms(AudioProcessing* ap, int delay) {
  // TODO: Need to mutex lock.
  ap->stream_delay_ms = delay;
}

void set_output_will_be_muted(AudioProcessing* ap, bool muted) {
  ap->processor->set_output_will_be_muted(muted);
}

void audio_processing_delete(AudioProcessing* ap) {
  delete ap;
}

bool is_success(const int code) {
  return code == webrtc::AudioProcessing::kNoError;
}

}  // namespace webrtc_audio_processing_wrapper
