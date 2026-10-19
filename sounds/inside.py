"""A building's deep hum, synthesised: mains hum at 50 Hz and its
harmonics, swelling slowly, a low rumble and a faint draught, in a room's
reverb: a dense tail of decaying, darkening noise (no early reflections,
which would be heard as an echo), convolved round the loop by FFT so the
tail wraps into the start and it loops without a seam. Writes inside.wav:
`nix-shell -p "python3.withPackages (p: [p.numpy])" --run "python3 inside.py"`,
then level it to bims' ambiences (-30 LUFS, measured with ffmpeg's ebur128):
`ffmpeg -i inside.wav -af volume=-12.4dB -c:a libvorbis -q:a 5 inside.ogg`.
"""
import wave

import numpy as np

RATE, LENGTH = 48000, 16
N = RATE * LENGTH
rng = np.random.default_rng(7)
t = np.arange(N) / RATE
freqs = np.fft.rfftfreq(N, 1 / RATE)


def shaped(lo, hi):
    """Noise between `lo` and `hi` Hz, soft-edged; periodic over the loop."""
    spec = np.fft.rfft(rng.standard_normal(N))
    gain = 1 / (1 + (lo / np.maximum(freqs, 1e-3)) ** 4) / (1 + (freqs / hi) ** 4)
    out = np.fft.irfft(spec * gain, N)
    return out / np.sqrt(np.mean(out ** 2))


swell = 1 + 0.12 * np.sin(2 * np.pi * t * 2 / LENGTH)
hum = sum(a * np.sin(2 * np.pi * f * t + p)
          for f, a, p in [(50, 1.0, 0.0), (100, 0.45, 1.1), (150, 0.18, 2.3), (200, 0.07, 0.7)])
dry = 0.5 * swell * hum + 0.35 * shaped(20, 70) + 0.05 * shaped(250, 600)

# The room: 2.5 s of noise dying away (1.6 s to -60 dB), duller as it goes.
k = np.arange(int(2.5 * RATE)) / RATE
tail = rng.standard_normal(len(k)) * 10 ** (-3 * k / 1.6) * np.minimum(1, k / 0.02)
tail = np.convolve(tail, np.ones(24) / 24, mode="same")
room = np.zeros(N)
room[: len(tail)] = tail
wet = np.fft.irfft(np.fft.rfft(dry) * np.fft.rfft(room), N)
wet *= 0.6 * np.sqrt(np.mean(dry ** 2) / np.mean(wet ** 2))

mix = dry + wet
mix *= 0.89 / np.max(np.abs(mix))
with wave.open("inside.wav", "wb") as w:
    w.setnchannels(1)
    w.setsampwidth(2)
    w.setframerate(RATE)
    w.writeframes((mix * 32767).astype("<i2").tobytes())
