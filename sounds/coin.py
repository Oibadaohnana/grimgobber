"""A coin landing, synthesised: a bright metal ping with inharmonic
overtones, then two quicker, quieter bounces. Writes coin.wav;
`ffmpeg -i coin.wav -c:a libvorbis -q:a 4 coin.ogg` makes what the game embeds.
"""
import math
import struct
import wave

RATE, LENGTH = 48000, 0.9
out = [0.0] * int(RATE * LENGTH)
# A small disc rings at these ratios to its lowest mode.
PARTIALS = [(1.0, 1.0), (2.32, 0.6), (4.25, 0.35), (6.63, 0.2)]


def ping(start, base, gain, decay):
    s = int(start * RATE)
    for i in range(len(out) - s):
        t = i / RATE
        v = sum(a * math.sin(2 * math.pi * base * r * t) * math.exp(-t / (decay / r ** 0.5))
                for r, a in PARTIALS)
        out[s + i] += gain * v * min(1.0, t * 4000)  # no click at the onset


for start, gain, decay in [(0.0, 1.0, 0.22), (0.11, 0.45, 0.12), (0.18, 0.25, 0.08)]:
    ping(start, 2300.0, gain, decay)

peak = max(abs(v) for v in out)
with wave.open("coin.wav", "wb") as w:
    w.setnchannels(1)
    w.setsampwidth(2)
    w.setframerate(RATE)
    w.writeframes(b"".join(struct.pack("<h", int(v / peak * 0.89 * 32767)) for v in out))
