/* SPDX-License-Identifier: LGPL-3.0-or-later WITH LGPL-3.0-linking-exception
 *
 * `libasound_module_pcm_aera.so`: an ALSA playback PCM that plays through
 * AERA's audio bridge, so anything that speaks ALSA (miniaudio and with it
 * flutter_soloud, SDL, GStreamer's alsasink, raw snd_pcm_writei) plays
 * sound in AERA unchanged.
 *
 * The bridge is AERA's closed `/system/bin/aera-audio-bridge`, shipped only
 * by official AERA builds. aera-flutter starts it as AERA's own features do
 * (src/audio.rs); this module connects the way AERA's browser sink does
 * (aeraui/features/browser/gst_aera_audio_sink.c): a stream socket to the
 * abstract name `aera-browser-audio-v1`, a peer that must be root, a
 * four-word hello ("APRA", 48000, 2, 16, little-endian) and then
 * interleaved S16LE 48 kHz stereo until close. Without the bridge, opening
 * the device fails with ENODEV.
 *
 * Only that format is offered: the `plug` PCM in front of it (aera.conf)
 * converts. One connection per open PCM; the socket's send queue paces the
 * writer and is what snd_pcm_delay reports.
 */

#define _GNU_SOURCE
#include <alsa/asoundlib.h>
#include <alsa/pcm_external.h>
#include <errno.h>
#include <limits.h>
#include <linux/sockios.h>
#include <poll.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <time.h>
#include <unistd.h>

#define BRIDGE "/system/bin/aera-audio-bridge"
#define SOCKET_NAME "aera-browser-audio-v1"
#define RATE 48000
#define CHANNELS 2
#define FRAME_BYTES 4
/* About 40 ms queued in the socket before writers block. */
#define SEND_BUFFER (RATE * FRAME_BYTES / 25)

typedef struct {
  snd_pcm_ioplug_t io;
  int fd;
  snd_pcm_uframes_t hw_ptr;
  snd_pcm_uframes_t boundary;
} aera_t;

static void sleep_ms(long ms) {
  struct timespec t = {ms / 1000, (ms % 1000) * 1000000L};
  while (nanosleep(&t, &t) && errno == EINTR) {
  }
}

static int send_all(int fd, const void *bytes, size_t size) {
  const char *p = bytes;
  while (size) {
    ssize_t n = send(fd, p, size, MSG_NOSIGNAL);
    if (n < 0 && errno == EINTR) continue;
    if (n <= 0) return -EPIPE;
    p += n;
    size -= (size_t)n;
  }
  return 0;
}

/* Connects to the bridge. aera-flutter starts it at launch, so retry
 * briefly in case it is not listening yet. */
static int connect_bridge(void) {
  if (access(BRIDGE, X_OK)) return -ENODEV;
  struct sockaddr_un address;
  memset(&address, 0, sizeof(address));
  address.sun_family = AF_UNIX;
  memcpy(address.sun_path + 1, SOCKET_NAME, sizeof(SOCKET_NAME) - 1);
  const socklen_t size = (socklen_t)(offsetof(struct sockaddr_un, sun_path) + 1 + sizeof(SOCKET_NAME) - 1);
  for (int attempt = 0;; ++attempt) {
    int fd = socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC, 0);
    if (fd < 0) return -errno;
    if (!connect(fd, (const struct sockaddr *)&address, size)) {
      struct ucred peer;
      socklen_t peer_size = sizeof(peer);
      const uint32_t hello[4] = {htole32(0x41525041U), htole32(RATE), htole32(CHANNELS), htole32(16)};
      const int buffer = SEND_BUFFER;
      setsockopt(fd, SOL_SOCKET, SO_SNDBUF, &buffer, sizeof(buffer));
      if (getsockopt(fd, SOL_SOCKET, SO_PEERCRED, &peer, &peer_size) || peer_size != sizeof(peer) || peer.uid != 0 ||
          send_all(fd, hello, sizeof(hello))) {
        close(fd);
        return -EACCES;
      }
      return fd;
    }
    const int error = errno;
    close(fd);
    if ((error != ECONNREFUSED && error != ENOENT) || attempt == 20) return -ENODEV;
    sleep_ms(50);
  }
}

static int aera_start(snd_pcm_ioplug_t *io) {
  (void)io;
  return 0;
}

static int aera_stop(snd_pcm_ioplug_t *io) {
  (void)io;
  return 0;
}

static int aera_prepare(snd_pcm_ioplug_t *io) {
  aera_t *a = io->private_data;
  a->hw_ptr = 0;
  /* ALSA's boundary for this buffer size (pcm.c, snd_pcm_hw_params). */
  a->boundary = io->buffer_size;
  while (a->boundary * 2 <= LONG_MAX - io->buffer_size) a->boundary *= 2;
  return 0;
}

/* Everything written is already in the socket: the ring is always free,
 * and the socket blocks the writer when the bridge falls behind. */
static snd_pcm_sframes_t aera_pointer(snd_pcm_ioplug_t *io) {
  aera_t *a = io->private_data;
  return (snd_pcm_sframes_t)a->hw_ptr;
}

static snd_pcm_sframes_t aera_transfer(snd_pcm_ioplug_t *io, const snd_pcm_channel_area_t *areas,
                                       snd_pcm_uframes_t offset, snd_pcm_uframes_t size) {
  aera_t *a = io->private_data;
  const char *bytes = (const char *)areas->addr + (areas->first + areas->step * offset) / 8;
  size_t left = size * FRAME_BYTES;
  while (left) {
    ssize_t n = send(a->fd, bytes, left, MSG_NOSIGNAL | (io->nonblock ? MSG_DONTWAIT : 0));
    if (n < 0 && errno == EINTR) continue;
    if (n < 0 && (errno == EAGAIN || errno == EWOULDBLOCK)) break;
    if (n <= 0) return -EPIPE;
    bytes += n;
    left -= (size_t)n;
  }
  /* The bridge reads whole frames; keep a partial frame's tail with it. */
  size_t sent = size * FRAME_BYTES - left;
  if (sent % FRAME_BYTES) {
    const size_t tail = FRAME_BYTES - sent % FRAME_BYTES;
    if (send_all(a->fd, bytes, tail)) return -EPIPE;
    sent += tail;
  }
  const snd_pcm_uframes_t frames = sent / FRAME_BYTES;
  if (!frames) return -EAGAIN;
  a->hw_ptr = (a->hw_ptr + frames) % a->boundary;
  return (snd_pcm_sframes_t)frames;
}

static int aera_delay(snd_pcm_ioplug_t *io, snd_pcm_sframes_t *delay) {
  aera_t *a = io->private_data;
  int queued = 0;
  if (ioctl(a->fd, SIOCOUTQ, &queued)) queued = 0;
  *delay = queued / FRAME_BYTES;
  return 0;
}

/* Waits (at most a second) for the socket to hand everything to the bridge. */
static int aera_drain(snd_pcm_ioplug_t *io) {
  aera_t *a = io->private_data;
  for (int i = 0; i < 100; ++i) {
    int queued = 0;
    if (ioctl(a->fd, SIOCOUTQ, &queued) || queued <= 0) break;
    sleep_ms(10);
  }
  return 0;
}

static int aera_poll_count(snd_pcm_ioplug_t *io) {
  (void)io;
  return 1;
}

static int aera_poll_descriptors(snd_pcm_ioplug_t *io, struct pollfd *pfd, unsigned int space) {
  aera_t *a = io->private_data;
  if (space < 1) return -EINVAL;
  pfd->fd = a->fd;
  pfd->events = POLLOUT;
  pfd->revents = 0;
  return 1;
}

static int aera_poll_revents(snd_pcm_ioplug_t *io, struct pollfd *pfd, unsigned int nfds, unsigned short *revents) {
  (void)io;
  if (nfds < 1) return -EINVAL;
  *revents = pfd->revents & (POLLOUT | POLLERR | POLLHUP);
  return 0;
}

static int aera_close(snd_pcm_ioplug_t *io) {
  aera_t *a = io->private_data;
  close(a->fd);
  free(a);
  return 0;
}

static const snd_pcm_ioplug_callback_t callbacks = {
    .start = aera_start,
    .stop = aera_stop,
    .pointer = aera_pointer,
    .transfer = aera_transfer,
    .close = aera_close,
    .prepare = aera_prepare,
    .drain = aera_drain,
    .poll_descriptors_count = aera_poll_count,
    .poll_descriptors = aera_poll_descriptors,
    .poll_revents = aera_poll_revents,
    .delay = aera_delay,
};

SND_PCM_PLUGIN_DEFINE_FUNC(aera) {
  (void)root;
  snd_config_iterator_t i, next;
  snd_config_for_each(i, next, conf) {
    snd_config_t *n = snd_config_iterator_entry(i);
    const char *id;
    if (snd_config_get_id(n, &id) < 0) continue;
    if (!strcmp(id, "comment") || !strcmp(id, "type") || !strcmp(id, "hint")) continue;
    SNDERR("Unknown field %s", id);
    return -EINVAL;
  }
  if (stream != SND_PCM_STREAM_PLAYBACK) return -ENODEV;

  aera_t *a = calloc(1, sizeof(*a));
  if (!a) return -ENOMEM;
  a->fd = connect_bridge();
  if (a->fd < 0) {
    const int error = a->fd;
    free(a);
    return error;
  }
  a->io.version = SND_PCM_IOPLUG_VERSION;
  a->io.name = "AERA audio bridge";
  a->io.callback = &callbacks;
  a->io.private_data = a;
  a->io.poll_fd = a->fd;
  a->io.poll_events = POLLOUT;
  a->io.mmap_rw = 0;
  a->io.flags = SND_PCM_IOPLUG_FLAG_BOUNDARY_WA;

  int err = snd_pcm_ioplug_create(&a->io, name, stream, mode);
  if (err < 0) {
    close(a->fd);
    free(a);
    return err;
  }
  /* MMAP too: plug's converters write their slave through mmap, which
   * ioplug emulates with its own buffer and transfer. */
  static const unsigned int access[] = {SND_PCM_ACCESS_RW_INTERLEAVED, SND_PCM_ACCESS_MMAP_INTERLEAVED};
  static const unsigned int format[] = {SND_PCM_FORMAT_S16_LE};
  if ((err = snd_pcm_ioplug_set_param_list(&a->io, SND_PCM_IOPLUG_HW_ACCESS, 2, access)) < 0 ||
      (err = snd_pcm_ioplug_set_param_list(&a->io, SND_PCM_IOPLUG_HW_FORMAT, 1, format)) < 0 ||
      (err = snd_pcm_ioplug_set_param_minmax(&a->io, SND_PCM_IOPLUG_HW_CHANNELS, CHANNELS, CHANNELS)) < 0 ||
      (err = snd_pcm_ioplug_set_param_minmax(&a->io, SND_PCM_IOPLUG_HW_RATE, RATE, RATE)) < 0 ||
      (err = snd_pcm_ioplug_set_param_minmax(&a->io, SND_PCM_IOPLUG_HW_PERIOD_BYTES, 256, 64 * 1024)) < 0 ||
      (err = snd_pcm_ioplug_set_param_minmax(&a->io, SND_PCM_IOPLUG_HW_PERIODS, 2, 64)) < 0) {
    snd_pcm_ioplug_delete(&a->io);
    return err;
  }
  *pcmp = a->io.pcm;
  return 0;
}

SND_PCM_PLUGIN_SYMBOL(aera);
