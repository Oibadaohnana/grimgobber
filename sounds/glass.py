"""A window smashed, synthesised (neither old game has one): a sharp crack,
a crunch, a low thud, then shards ringing as they land. Writes glass.wav;
`ffmpeg -i glass.wav -c:a libvorbis -q:a 4 glass.ogg` makes what the game embeds.
"""
import math
import random
import struct
import wave

RATE, LENGTH = 48000, 1.3
random.seed(7)
out = [0.0] * int(RATE * LENGTH)


def add(start, length, f):
    """Mixes f(t) into the output from `start` seconds for `length`."""
    s = int(start * RATE)
    for i in range(int(length * RATE)):
        if s + i < len(out):
            out[s + i] += f(i / RATE)


def noise_burst(start, length, decay, gain, bright):
    last = [0.0]

    def f(t):
        x = random.uniform(-1, 1)
        y = x - bright * last[0]  # a first difference: hiss, not rumble
        last[0] = x
        return y * gain * math.exp(-t / decay)

    add(start, length, f)


# The crack, the crunch after it, and the frame's thud.
noise_burst(0.0, 0.06, 0.010, 0.9, 0.95)
noise_burst(0.004, 0.25, 0.060, 0.35, 0.8)
add(0.0, 0.25, lambda t: 0.35 * math.sin(2 * math.pi * 85 * t) * math.exp(-t / 0.05))

# Shards: most early, fewer and quieter later, each a bright ping with an
# inharmonic overtone and a tick where it lands.
for _ in range(70):
    t0 = 0.02 + 1.0 * random.random() ** 2
    f0 = random.uniform(2500, 9000)
    tau = random.uniform(0.015, 0.06)
    g = random.uniform(0.04, 0.22) * (1.1 - t0 * 0.8)
    add(t0, 5 * tau, lambda t, f0=f0, tau=tau, g=g: g * math.exp(-t / tau)
        * (math.sin(2 * math.pi * f0 * t) + 0.4 * math.sin(2 * math.pi * f0 * 2.76 * t)))
    noise_burst(t0, 0.004, 0.0012, g * 1.5, 0.9)

peak = max(abs(v) for v in out)
with wave.open("glass.wav", "wb") as w:
    w.setnchannels(1)
    w.setsampwidth(2)
    w.setframerate(RATE)
    w.writeframes(b"".join(struct.pack("<h", int(v / peak * 0.89 * 32767)) for v in out))
