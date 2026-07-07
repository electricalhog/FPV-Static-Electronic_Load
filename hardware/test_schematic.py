"""
Schematic completeness tests for FPV Static Electronic Load.
Run:  pytest hardware/test_schematic.py -v
"""
import os, sys, glob
import pytest

SITE = '/home/daniel/.cache/uv/archive-v0/7cuypMzPSfPk6TCt/lib/python3.12/site-packages'
SCH  = '/home/daniel/GitHub/FPV-Static-Electronic_Load/hardware/electronic_load.kicad_sch'


def _find_symlib():
    """Same resolution as build_schematic.py: env var, flatpak (any hash), apt."""
    if p := os.environ.get('KICAD_SYMBOL_DIR'):
        return p
    probe = 'MCU_Module.kicad_sym'
    for m in reversed(sorted(glob.glob(
            '/var/lib/flatpak/runtime/org.kicad.KiCad.Library.Symbols'
            '/x86_64/stable/*/files/symbols'))):
        if os.path.exists(os.path.join(m, probe)):
            return m
    return '/usr/share/kicad/symbols'


SYMLIB = _find_symlib()
sys.path.insert(0, SITE)
os.environ['KICAD_SYMBOL_DIR']  = SYMLIB
os.environ['KICAD6_SYMBOL_DIR'] = SYMLIB

from kicad_sch_api import load_schematic


@pytest.fixture(scope='module')
def sch():
    return load_schematic(SCH)


# ── Net labels ──────────────────────────────────────────────────────────────

SIGNAL_NETS = [
    'ENC_A', 'ENC_B',
    'ISENSE1', 'ADC0',
    'ISENSE2', 'ADC2',
    'GATE_PWM', 'DISABLE',
    'SH1_P', 'SH1_N',
    'SH2_P', 'SH2_N',
]

SWD_NETS = ['SWDIO', 'SWDCLK', 'NRESET']

# Analog frontend (per-FET linear loops; see hardware/sim/)
ANALOG_NETS = [
    'VF1', 'VSET',                          # setpoint DAC filter
    'EIN1', 'EA1', 'NB1', 'PB1', 'GBUF1', 'GATE1',   # ch A discrete class-AB
    'EIN2', 'EA2', 'BOUT2', 'GATE2',                 # ch B BUF634A
    'VDUT',                                          # shared drain bus
]


def _label_counts(sch):
    from collections import Counter
    return Counter(l.text for l in sch.labels)


def test_signal_nets_each_appear_twice(sch):
    counts = _label_counts(sch)
    missing = [n for n in SIGNAL_NETS if counts[n] < 2]
    assert not missing, f"Nets with fewer than 2 label endpoints: {missing}"


def test_swd_nets_present(sch):
    counts = _label_counts(sch)
    missing = [n for n in SWD_NETS if counts[n] < 1]
    assert not missing, f"SWD net labels missing: {missing}"


def test_analog_nets_each_appear_twice(sch):
    counts = _label_counts(sch)
    missing = [n for n in ANALOG_NETS if counts[n] < 2]
    assert not missing, f"Analog nets with fewer than 2 label endpoints: {missing}"


def test_vdut_bus_reaches_both_fets_and_connector(sch):
    counts = _label_counts(sch)
    assert counts['VDUT'] >= 3, (
        f"VDUT should appear on Q7 drain, Q8 drain and J5: got {counts['VDUT']}"
    )


def test_both_gate_buffer_variants_present(sch):
    refs = {c.reference: c.value for c in sch.components.all()}
    # discrete class-AB variant
    assert refs.get('Q5') == 'BD139' and refs.get('Q6') == 'BD140'
    # all-in-one variant
    assert refs.get('U8', '').startswith('BUF634')
    # both power FETs and TGHG shunts
    assert refs.get('Q7') == 'IXFN360N10T' and refs.get('Q8') == 'IXFN360N10T'
    assert refs.get('R26') == '2m TGHG' and refs.get('R29') == '2m TGHG'


# ── Power symbols ───────────────────────────────────────────────────────────

def test_power_symbols_use_pwr_reference(sch):
    bad = [
        comp.reference
        for comp in sch.components.all()
        if comp.lib_id.startswith('power:')
        and not comp.reference.startswith('#PWR')
    ]
    assert not bad, f"Power symbols with wrong reference prefix: {bad}"


# ── U1 Pico power/control pins ──────────────────────────────────────────────

def _power_lib_ids_at(sch, x, y, tol=0.5):
    return [
        c.lib_id for c in sch.components.all()
        if c.lib_id.startswith('power:')
        and abs(c.position.x - x) < tol
        and abs(c.position.y - y) < tol
    ]


def test_u1_vsys_has_power_symbol(sch):
    # U1 pin 39 (VSYS) must have a power symbol placed at its position
    pos = sch.get_component_pin_position('U1', '39')
    found = _power_lib_ids_at(sch, pos.x, pos.y)
    assert found, f"No power symbol at U1 VSYS pin ({pos.x}, {pos.y})"


def test_u1_run_has_nreset_label(sch):
    # U1 pin 30 (RUN) should be driven by NRESET label or tied to 3V3
    pos = sch.get_component_pin_position('U1', '30')
    labels_here = [
        l.text for l in sch.labels
        if abs(l.position.x - pos.x) < 0.5 and abs(l.position.y - pos.y) < 0.5
    ]
    pwr_here = _power_lib_ids_at(sch, pos.x, pos.y)
    assert labels_here or pwr_here, (
        f"U1 RUN pin ({pos.x}, {pos.y}) has neither a label nor a power symbol"
    )


def test_u1_remaining_gnd_pins_connected(sch):
    # Passive GND pins: 8, 13, 18, 23, 28, 38
    gnd_pins = ['8', '13', '18', '23', '28', '38']
    missing = []
    for pnum in gnd_pins:
        pos = sch.get_component_pin_position('U1', pnum)
        found = _power_lib_ids_at(sch, pos.x, pos.y)
        if not found:
            missing.append(pnum)
    assert not missing, f"U1 GND pins without power symbols: {missing}"


# ── RC filter wires ─────────────────────────────────────────────────────────

def test_rc_filter_wires_exist(sch):
    wires = list(sch.wires)
    assert len(wires) >= 2, "Expected ≥2 RC filter wires (R→C stubs)"


def test_rc_filter_wires_are_vertical(sch):
    for w in sch.wires:
        pts = w.points
        assert abs(pts[0].x - pts[1].x) < 0.01, (
            f"RC filter wire should be vertical, got x={pts[0].x} vs {pts[1].x}"
        )
