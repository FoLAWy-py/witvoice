"""Model-independent report checks; no Torch imports or hardware use."""
import math


def module_devices(module):
    devices = sorted({str(value.device) for getter in (module.parameters, module.buffers)
                      for value in getter()})
    return {"devices": devices, "actual": devices[0] if len(devices) == 1 else
            "MIXED" if devices else "UNKNOWN"}


def paced_statistics(steps, rate):
    if not steps or rate <= 0 or any(s["new_samples"] <= 0 or
            not all(math.isfinite(s[k]) and s[k] >= 0 for k in ("processing_ms", "lag_ms")) for s in steps):
        raise ValueError("invalid step evidence")
    def percentile(values, fraction):
        return sorted(values)[math.ceil(len(values) * fraction) - 1]
    durations = [s["processing_ms"] for s in steps]
    ratios = [s["processing_ms"] / (s["new_samples"] / rate * 1000) for s in steps]
    lags = [s["lag_ms"] / (s["new_samples"] / rate * 1000) for s in steps]
    seconds = sum(s["new_samples"] for s in steps) / rate
    return {"sample_count": len(steps), "average_rtf": sum(durations) / (seconds * 1000),
            "p50_step_ms": percentile(durations, .5), "p95_step_ms": percentile(durations, .95),
            "p99_step_ms": percentile(durations, .99), "p99_step_rtf": percentile(ratios, .99),
            "new_input_seconds": seconds, "max_lag_ms": max(s["lag_ms"] for s in steps),
            "max_lag_chunk_ratio": max(lags)}


def require_safe_torch(version):
    import re
    match = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)(?:\+[a-zA-Z0-9.]+)?", version)
    if not match or tuple(map(int, match.groups())) < (2, 10, 0):
        raise RuntimeError("Torch <2.10.0/unknown blocked: CVE-2025-32434 and CVE-2026-24747; original measurements remain historical only")
