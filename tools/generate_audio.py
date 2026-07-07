#!/usr/bin/env python3
"""Procedurally generate the game's sound effects.

Regenerates every .wav under crates/factory_app/assets/audio/. Deterministic
(fixed RNG seed), so the assets can be reproduced or tweaked from source
instead of being opaque binaries.

Usage: python3 tools/generate_audio.py
Requires: numpy
"""

import os
import wave

import numpy as np

SR = 44100
OUT_DIR = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
    "crates",
    "factory_app",
    "assets",
    "audio",
)

# Reseeded per sound from its name (see main()), so each generator's output
# is independent of SOUNDS ordering and of rng usage in other generators.
rng = np.random.default_rng(0xFAC70)


# --- helpers ---------------------------------------------------------------


def t(dur):
    return np.arange(int(round(dur * SR))) / SR


def exp_env(dur, tau, delay=0.0):
    """Exponential decay envelope, optionally delayed."""
    tt = t(dur) - delay
    return np.where(tt >= 0.0, np.exp(-np.maximum(tt, 0.0) / tau), 0.0)


def attack(x, ms):
    """Linear fade-in over the first `ms` milliseconds (declicks the onset)."""
    n = min(len(x), int(SR * ms / 1000))
    if n > 0:
        x[:n] *= np.linspace(0.0, 1.0, n)
    return x


def fade_out(x, ms):
    n = min(len(x), int(SR * ms / 1000))
    if n > 0:
        x[-n:] *= np.linspace(1.0, 0.0, n)
    return x


def fft_filter(x, lo=None, hi=None, slope=2.0):
    """Butterworth-magnitude band filter via the frequency domain.

    Circular by construction, so filtered noise wraps around seamlessly —
    exactly what the machine loops need.
    """
    spectrum = np.fft.rfft(x)
    f = np.fft.rfftfreq(len(x), 1 / SR)
    resp = np.ones_like(f)
    if lo is not None:
        resp *= 1.0 / np.sqrt(1.0 + (lo / np.maximum(f, 1e-9)) ** (2 * slope))
    if hi is not None:
        resp *= 1.0 / np.sqrt(1.0 + (f / hi) ** (2 * slope))
    return np.fft.irfft(spectrum * resp, len(x))


def sweep_lowpass(x, f_start, f_end):
    """One-pole lowpass whose cutoff sweeps exponentially over the signal."""
    n = len(x)
    cutoff = f_start * (f_end / f_start) ** (np.arange(n) / max(n - 1, 1))
    alpha = 1.0 - np.exp(-2.0 * np.pi * cutoff / SR)
    out = np.empty(n)
    state = 0.0
    for i in range(n):
        state += alpha[i] * (x[i] - state)
        out[i] = state
    return out


def normalize(x, peak=0.8):
    m = np.max(np.abs(x))
    return x * (peak / m) if m > 0 else x


def bell(freq, dur, tau, brightness=1.0):
    """Struck-bell voice from inharmonic partials with faster-decaying highs."""
    tt = t(dur)
    partials = (
        (1.0, 1.0, 1.0),
        (2.0, 0.55, 0.55),
        (2.76, 0.30 * brightness, 0.35),
        (4.07, 0.14 * brightness, 0.22),
        (5.43, 0.07 * brightness, 0.14),
    )
    out = np.zeros_like(tt)
    for ratio, amp, tau_scale in partials:
        if freq * ratio < SR / 2:
            out += amp * np.sin(2 * np.pi * freq * ratio * tt) * np.exp(
                -tt / (tau * tau_scale)
            )
    return attack(out, 3)


def place_at(buf, x, at_s, gain=1.0):
    """Mix `x` into `buf` starting at `at_s` seconds, clipping to fit."""
    i = int(at_s * SR)
    n = min(len(x), len(buf) - i)
    if n > 0:
        buf[i : i + n] += x[:n] * gain


def write_wav(name, x):
    os.makedirs(OUT_DIR, exist_ok=True)
    path = os.path.join(OUT_DIR, name)
    data = (np.clip(x, -1.0, 1.0) * 32767).astype("<i2")
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(data.tobytes())
    print(f"  {name}: {len(x) / SR:.2f}s, peak {np.max(np.abs(x)):.2f}")


# --- one-shots ---------------------------------------------------------------


def ui_click():
    """Soft, tight tick: tiny noise transient plus a high sine tap."""
    dur = 0.07
    tt = t(dur)
    noise = fft_filter(rng.standard_normal(len(tt)), lo=2500, hi=9000)
    x = noise * exp_env(dur, 0.006)
    x += 0.5 * np.sin(2 * np.pi * 1900 * tt) * exp_env(dur, 0.012)
    x += 0.25 * np.sin(2 * np.pi * 620 * tt) * exp_env(dur, 0.02)
    return fade_out(normalize(attack(x, 1)), 10)


def place():
    """Chunky mechanical placement: pitch-drop thump, clank, latch settle."""
    dur = 0.34
    tt = t(dur)
    x = np.zeros_like(tt)

    # Body thump with a fast downward pitch sweep.
    f_inst = 150.0 * np.exp(-tt / 0.045) + 52.0
    phase = 2 * np.pi * np.cumsum(f_inst) / SR
    x += 1.0 * np.sin(phase) * exp_env(dur, 0.075)

    # Impact clank: band-limited noise burst.
    clank = fft_filter(rng.standard_normal(len(tt)), lo=500, hi=3200)
    x += 0.55 * clank * exp_env(dur, 0.02)

    # Metallic ring: quiet inharmonic partials.
    for freq, amp in ((1230, 0.10), (1972, 0.07), (2643, 0.05), (3477, 0.03)):
        x += amp * np.sin(2 * np.pi * freq * tt) * exp_env(dur, 0.05)

    # Latch settling: a small delayed secondary tap.
    tap = fft_filter(rng.standard_normal(len(tt)), lo=700, hi=2500)
    x += 0.22 * tap * exp_env(dur, 0.012, delay=0.095)
    x += 0.18 * np.sin(2 * np.pi * 95 * tt) * exp_env(dur, 0.03, delay=0.095)

    return fade_out(normalize(attack(x, 1)), 30)


def place_error():
    """Denial buzz: two rough, dissonant low tones ("uh-uh")."""
    dur = 0.36
    x = np.zeros(len(t(dur)))
    for start, freq in ((0.0, 196.0), (0.16, 165.0)):
        seg_t = t(0.13)
        tone = np.sin(2 * np.pi * freq * seg_t)
        tone += 0.35 * np.sin(2 * np.pi * freq * 3 * seg_t)
        tone += 0.15 * np.sin(2 * np.pi * freq * 5 * seg_t)
        tone *= 1.0 + 0.5 * np.sin(2 * np.pi * 33 * seg_t)  # AM roughness
        tone *= np.minimum(seg_t / 0.008, 1.0) * np.exp(-seg_t / 0.09)
        place_at(x, fade_out(tone, 15), start)
    return fade_out(normalize(fft_filter(x, hi=3800)), 20)


def manual_mine_tick():
    """Pickaxe striking rock: sharp crack, stony body, a little gravel."""
    dur = 0.16
    tt = t(dur)
    crack = fft_filter(rng.standard_normal(len(tt)), lo=1400, hi=9500)
    x = crack * exp_env(dur, 0.008)
    body = fft_filter(rng.standard_normal(len(tt)), lo=250, hi=900)
    x += 0.7 * body * exp_env(dur, 0.02)
    for _ in range(4):  # gravel debris
        grain_t = t(0.02)
        grain = fft_filter(rng.standard_normal(len(grain_t)), lo=900, hi=5000)
        grain *= np.exp(-grain_t / 0.004)
        place_at(x, grain, rng.uniform(0.02, 0.09), gain=0.25)
    return fade_out(normalize(attack(x, 1)), 20)


def manual_mine_complete():
    """Rock breaking apart: crack, darkening crumble, scattering pebbles."""
    dur = 0.5
    tt = t(dur)
    x = np.zeros_like(tt)

    crack = fft_filter(rng.standard_normal(len(tt)), lo=1000, hi=8000)
    x += crack * exp_env(dur, 0.012)
    x += 0.6 * np.sin(2 * np.pi * 70 * tt) * exp_env(dur, 0.06)  # low thud

    # Crumble: noise through a lowpass sweeping dark, fading out.
    crumble = sweep_lowpass(rng.standard_normal(len(tt)), 4000, 350)
    x += 2.2 * crumble * exp_env(dur, 0.13, delay=0.01)

    for i in range(6):  # pebbles scattering, getting quieter
        grain_t = t(0.025)
        lo = rng.uniform(700, 1800)
        grain = fft_filter(rng.standard_normal(len(grain_t)), lo=lo, hi=lo * 4)
        grain *= np.exp(-grain_t / 0.005)
        place_at(x, grain, rng.uniform(0.08, 0.4), gain=0.3 * 0.82**i)

    return fade_out(normalize(attack(x, 1)), 60)


def craft_complete():
    """Warm two-note completion chime (D5 up to G5)."""
    dur = 0.55
    x = np.zeros(len(t(dur)))
    place_at(x, bell(587.33, 0.30, 0.09, brightness=0.7), 0.0, gain=0.8)
    place_at(x, bell(783.99, 0.47, 0.13, brightness=0.8), 0.08)
    return fade_out(normalize(x), 60)


def research_complete():
    """Ascending bell fanfare (C major arpeggio) with a shimmer tail."""
    dur = 1.5
    tt = t(dur)
    x = np.zeros_like(tt)
    for i, freq in enumerate((523.25, 659.25, 783.99, 1046.5)):
        last = i == 3
        note_dur = 1.1 if last else 0.35
        tau = 0.28 if last else 0.10
        place_at(x, bell(freq, note_dur, tau), i * 0.11, gain=0.6 + 0.13 * i)
    # Soft root pad underneath and a high shimmer on the resolution.
    x += 0.12 * np.sin(2 * np.pi * 261.63 * tt) * exp_env(dur, 0.4) * np.minimum(
        tt / 0.05, 1.0
    )
    vibrato = np.sin(2 * np.pi * 2093 * tt + 3.0 * np.sin(2 * np.pi * 5 * tt))
    x += 0.05 * vibrato * exp_env(dur, 0.35, delay=0.33)
    return fade_out(normalize(x), 120)


# --- machine loops ------------------------------------------------------------
# Tonal components use frequencies that are integer multiples of 1/duration and
# noise is shaped with circular FFT filters, so both loops wrap seamlessly.


def machine_burner_loop():
    """Burner rumble: fire bed, flame flicker, sparse crackles."""
    dur = 3.0
    tt = t(dur)

    # Fire bed: brown-ish noise kept low.
    bed = np.cumsum(rng.standard_normal(len(tt)))
    bed -= np.linspace(bed[0], bed[-1], len(bed))  # loop-safe detrend
    bed = fft_filter(bed - bed.mean(), lo=25, hi=180, slope=1.5)
    bed = normalize(bed, 1.0)

    # Flicker: AM at integer cycles per loop.
    flicker = (
        1.0
        + 0.22 * np.sin(2 * np.pi * (2 / dur) * tt)
        + 0.14 * np.sin(2 * np.pi * (5 / dur) * tt + 1.3)
        + 0.10 * np.sin(2 * np.pi * (9 / dur) * tt + 4.1)
    )
    x = bed * flicker

    # Airy hiss of the flame itself, gently flickering too.
    hiss = fft_filter(rng.standard_normal(len(tt)), lo=400, hi=2400)
    x += 0.12 * normalize(hiss, 1.0) * (1.0 + 0.3 * np.sin(2 * np.pi * (7 / dur) * tt))

    # Crackles: short bursts fully contained inside the loop.
    for _ in range(22):
        grain_t = t(rng.uniform(0.004, 0.012))
        grain = fft_filter(rng.standard_normal(len(grain_t)), lo=1800, hi=8000)
        grain *= np.exp(-grain_t / 0.0025)
        at = rng.uniform(0.0, dur - 0.05)
        place_at(x, grain, at, gain=rng.uniform(0.15, 0.45))

    return normalize(x, 0.7)


def machine_electric_loop():
    """Electric hum with a mechanical whir, seamless over 2 s."""
    dur = 2.0
    tt = t(dur)

    # Mains-style hum stack (all integer Hz -> integer cycles in 2 s).
    # Random phases keep the harmonics' slope maxima from stacking up.
    x = np.zeros_like(tt)
    for freq, amp in ((100, 1.0), (200, 0.45), (300, 0.22), (400, 0.10)):
        x += amp * np.sin(2 * np.pi * freq * tt + rng.uniform(0, 2 * np.pi))
    x *= 1.0 + 0.08 * np.sin(2 * np.pi * (3 / dur) * tt)  # slow load wobble

    # Mechanical whir: bandpassed noise with a rotational AM.
    whir = fft_filter(rng.standard_normal(len(tt)), lo=700, hi=1800)
    whir = normalize(whir, 1.0) * (1.0 + 0.35 * np.sin(2 * np.pi * 8 * tt))
    x += 0.30 * whir

    # Faint high whine.
    x += 0.04 * np.sin(2 * np.pi * 3000 * tt) * (
        1.0 + 0.5 * np.sin(2 * np.pi * (5 / dur) * tt)
    )

    return normalize(x, 0.7)


SOUNDS = {
    "ui_click.wav": ui_click,
    "place.wav": place,
    "place_error.wav": place_error,
    "manual_mine_tick.wav": manual_mine_tick,
    "manual_mine_complete.wav": manual_mine_complete,
    "craft_complete.wav": craft_complete,
    "research_complete.wav": research_complete,
    "machine_burner_loop.wav": machine_burner_loop,
    "machine_electric_loop.wav": machine_electric_loop,
}


def main():
    global rng
    print(f"Writing to {OUT_DIR}")
    for name, generator in SOUNDS.items():
        rng = np.random.default_rng([0xFAC70, *name.encode()])
        write_wav(name, generator())


if __name__ == "__main__":
    main()
