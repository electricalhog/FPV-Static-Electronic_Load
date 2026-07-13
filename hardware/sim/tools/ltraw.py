#!/usr/bin/env python3
"""Minimal LTspice .raw (binary/ascii) reader -> dict of numpy arrays.

Handles AC (complex), DC/OP/TRAN (real). Returns (data, meta) where
data[varname] = np.ndarray and meta has plotname, nvars, npoints, flags.
The first variable is the sweep axis (freq/time/sweep).
"""
import numpy as np, re, sys


def read_raw(path):
    with open(path, "rb") as f:
        raw = f.read()
    # Header is ASCII (possibly UTF-16LE on some LTspice builds) up to 'Binary:'/'Values:'
    # Detect UTF-16LE by NUL bytes in the first chunk.
    head_end_bin = raw.find(b"Binary:\n")
    head_end_val = raw.find(b"Values:\n")
    if head_end_bin == -1 and head_end_val == -1:
        # try UTF-16
        txt = raw.decode("utf-16-le", errors="ignore")
        is_utf16 = True
    else:
        txt = None
        is_utf16 = False

    if is_utf16:
        # locate markers in decoded text
        bpos = txt.find("Binary:\n")
        vpos = txt.find("Values:\n")
        header = txt[: (bpos if bpos != -1 else vpos)]
        binary_mode = bpos != -1
        # byte offset of data start
        marker = "Binary:\n" if binary_mode else "Values:\n"
        data_char = (bpos if binary_mode else vpos) + len(marker)
        data_bytes = raw[data_char * 2 :]
    else:
        binary_mode = head_end_bin != -1
        pos = head_end_bin if binary_mode else head_end_val
        header = raw[:pos].decode("latin-1")
        marker = b"Binary:\n" if binary_mode else b"Values:\n"
        data_bytes = raw[pos + len(marker):]

    meta = {}
    for key in ["Plotname", "Flags", "No. Variables", "No. Points"]:
        m = re.search(rf"{re.escape(key)}:\s*(.+)", header)
        if m:
            meta[key] = m.group(1).strip()
    nvars = int(meta["No. Variables"])
    npts = int(meta["No. Points"])
    flags = meta.get("Flags", "")
    complex_data = "complex" in flags
    meta["complex"] = complex_data
    meta["plotname"] = meta.get("Plotname", "")

    # variable names
    vblock = header[header.find("Variables:"):]
    names = []
    for line in vblock.splitlines()[1:]:
        parts = line.split()
        if len(parts) >= 3 and parts[0].isdigit():
            names.append(parts[1])
        if len(names) == nvars:
            break

    data = {n: np.zeros(npts, dtype=complex if complex_data else float) for n in names}

    if binary_mode:
        if complex_data:
            # AC: each point = nvars complex128 (16 bytes each)
            arr = np.frombuffer(data_bytes, dtype="<c16", count=nvars * npts)
            arr = arr.reshape(npts, nvars)
            for i, n in enumerate(names):
                data[n] = arr[:, i]
        else:
            # TRAN/DC: axis var is 8-byte double, others 4-byte float (LTspice)
            # Layout per point: double(axis) + float*(nvars-1)
            rec = np.dtype([("axis", "<f8")] + [(f"v{i}", "<f4") for i in range(nvars - 1)])
            arr = np.frombuffer(data_bytes, dtype=rec, count=npts)
            data[names[0]] = arr["axis"].astype(float)
            for i, n in enumerate(names[1:]):
                data[n] = arr[f"v{i}"].astype(float)
    else:
        # ASCII values
        toks = data_bytes.decode("latin-1").split()
        # format: idx  axisval  then nvars-1 values (complex as re,im with comma)
        it = iter(toks)
        # fallback simple parser
        vals = data_bytes.decode("latin-1")
        rows = re.split(r"\n(?=\d+\t|\d+ )", vals.strip())
        for p, row in enumerate(rows[:npts]):
            nums = row.replace(",", " ").split()
            # first token is point index
            nums = nums[1:]
            if complex_data:
                for i, n in enumerate(names):
                    re_, im_ = float(nums[2 * i]), float(nums[2 * i + 1])
                    data[n][p] = complex(re_, im_)
            else:
                for i, n in enumerate(names):
                    data[n][p] = float(nums[i])
    return data, meta


if __name__ == "__main__":
    d, m = read_raw(sys.argv[1])
    print(m)
    for k, v in d.items():
        print(f"  {k:20s} n={len(v)} first={v[0]} last={v[-1]}")
