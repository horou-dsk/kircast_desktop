#include <libavformat/avformat.h>

AVCodecParameters *ff_alac_par(uint8_t *data, size_t data_size)
{
  AVCodecParameters *par = avcodec_parameters_alloc();
  par->codec_type = AVMEDIA_TYPE_AUDIO;
  par->codec_id = AV_CODEC_ID_ALAC;
  par->channels = 2;
  uint8_t *hdata = av_malloc(data_size);
  memcpy(hdata, data, data_size);
  par->extradata = hdata;
  par->extradata_size = data_size;
  par->sample_rate = 48000;
  return par;
}

AVCodecParameters *ff_aac_par(uint8_t *data, size_t data_size)
{
  AVCodecParameters *par = avcodec_parameters_alloc();
  par->codec_type = AVMEDIA_TYPE_AUDIO;
  par->codec_id = AV_CODEC_ID_AAC;
  par->channels = 2;
  uint8_t *hdata = av_malloc(data_size);
  memcpy(hdata, data, data_size);
  par->extradata = hdata;
  par->extradata_size = data_size;
  par->sample_rate = 44100;
  return par;
}