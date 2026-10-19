"""An EMF reader's beep, synthesised: a short, slightly buzzy square-ish tone.
Writes beep.wav; `ffmpeg -i beep.wav -c:a libvorbis -q:a 4 beep.ogg` makes
what the game embeds.
"""
import math
import struct
import wave

RATE, LENGTH, PITCH = 48000, 0.11, 2600.0
n = int(RATE * LENGTH)
out = []
for i in range(n):
    t = i / RATE
    # Odd harmonics, for a small piezo's edge, faded in and out without clicks.
    v = sum(math.sin(2 * math.pi * PITCH * k * t) / k for k in (1, 3, 5))
    out.append(v * min(1.0, t * 2000, (LENGTH - t) * 400))

peak = max(abs(v) for v in out)
with wave.open("beep.wav", "wb") as w:
    w.setnchannels(1)
    w.setsampwidth(2)
    w.setframerate(RATE)
    w.writeframes(b"".join(struct.pack("<h", int(v / peak * 0.89 * 32767)) for v in out))
