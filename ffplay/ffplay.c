#include <libavformat/avformat.h>

AVCodecParameters *ff_audio_codec_par(enum AVCodecID codec_id, uint8_t *data, size_t data_size, int sample_rate, int channels)
{
  AVCodecParameters *par = avcodec_parameters_alloc();
  par->codec_type = AVMEDIA_TYPE_AUDIO;
  par->codec_id = codec_id;
  par->channels = channels;
  uint8_t *hdata = av_malloc(data_size);
  memcpy(hdata, data, data_size);
  par->extradata = hdata;
  par->extradata_size = data_size;
  par->sample_rate = sample_rate;
  return par;
}