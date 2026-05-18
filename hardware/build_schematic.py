#!/usr/bin/env python3
"""
Build the FPV Static Electronic Load schematic.

Two-pass strategy:
  Pass 1 – place all symbols and save (establishes embedded lib_symbols).
  Pass 2 – reload, calculate pin positions, add all wires / labels / power.

Layout (mm, 1.27 mm grid):
  U1  Pico          centre-left  (80, 100)
  U2  INA185 ch1    right        (185, 75)
  U3  INA185 ch2    right        (185, 120)
  SW1 encoder       far-left     (25, 100)
  R1  100 Ω series  horizontal   (155, 75)   rot=90
  R2  100 Ω series  horizontal   (155, 120)  rot=90
  C1  100 nF shunt  below R1-L   (151, 88)
  C2  100 nF shunt  below R2-L   (151, 133)
  C3  10 µF bulk    near Pico    (60, 55)
  C4  100 nF local  near Pico    (70, 55)
  C5  100 nF decoup near INA1    (175, 60)
  C6  100 nF decoup near INA2    (175, 105)
  J1  Shunt-1 4-pin far-right    (230, 70)
  J2  Shunt-2 4-pin far-right    (230, 115)
  J3  Gate drive    far-right    (230, 155)
  J4  SWD debug     far-left-bot (25, 145)
"""

import sys, os, glob
from pathlib import Path

# ── Symbol library resolution ─────────────────────────────────────────────────
# Priority order: env var → flatpak (any hash) → apt/snap install
_PROBE_LIB = 'MCU_Module.kicad_sym'   # must be present for a complete install

def _find_symlib():
    if p := os.environ.get('KICAD_SYMBOL_DIR'):
        return p
    # flatpak (glob so any runtime hash works; MCU_Module present since KiCad 6)
    matches = sorted(glob.glob(
        '/var/lib/flatpak/runtime/org.kicad.KiCad.Library.Symbols'
        '/x86_64/stable/*/files/symbols'))
    for m in reversed(matches):      # newest hash first
        if os.path.exists(os.path.join(m, _PROBE_LIB)):
            return m
    # apt/snap install: /usr/share/kicad/symbols (KiCad 7+ from PPA is complete)
    apt = '/usr/share/kicad/symbols'
    if os.path.exists(os.path.join(apt, _PROBE_LIB)):
        return apt
    raise RuntimeError(
        f"KiCad symbol library missing {_PROBE_LIB}; "
        "set KICAD_SYMBOL_DIR or install KiCad >= 7"
    )

SYMLIB = _find_symlib()
OUT    = str(Path(__file__).parent / 'electronic_load.kicad_sch')

# Add local uv cache site-packages only when kicad_sch_api isn't already importable
try:
    import kicad_sch_api  # noqa: F401 – already on path (CI, venv, etc.)
except ImportError:
    _local = '/home/daniel/.cache/uv/archive-v0/7cuypMzPSfPk6TCt/lib/python3.12/site-packages'
    sys.path.insert(0, _local)

os.environ.setdefault('KICAD_SYMBOL_DIR',  SYMLIB)
os.environ.setdefault('KICAD6_SYMBOL_DIR', SYMLIB)

from kicad_sch_api import create_schematic, load_schematic, SymbolLibraryCache

cache = SymbolLibraryCache()
cache.add_library_path(SYMLIB)
cache.discover_libraries()

# ─────────────────────────────────────────────────────────────────────────────
# PASS 1 – Symbol placement
# ─────────────────────────────────────────────────────────────────────────────

sch = create_schematic()
sch.set_paper_size('A2')
sch.set_title_block(
    title='FPV Static Electronic Load',
    company='',
    rev='A',
    date='2025-05-06',
    comments={1: 'RP2040 digital PI current controller'},
)

C = sch.components

def add(lib_sym, ref, val, x, y, rot=0, fp='', **kw):
    return C.add(lib_sym, reference=ref, value=val,
                 position=(x, y), rotation=rot, footprint=fp, **kw)

# Microcontroller
add('MCU_Module:RaspberryPi_Pico', 'U1', 'RaspberryPi_Pico',
    80, 100, fp='MCU_Module:RaspberryPi_Pico')

# Current sense amplifiers
add('Amplifier_Current:INA185', 'U2', 'INA185A1IDBVR',
    185, 75, fp='Package_TO_SOT_SMD:SOT-23-5')
add('Amplifier_Current:INA185', 'U3', 'INA185A1IDBVR',
    185, 120, fp='Package_TO_SOT_SMD:SOT-23-5')

# Rotary encoder
add('Device:RotaryEncoder_Switch', 'SW1', 'EC11',
    25, 100, fp='Rotary_Encoder:RotaryEncoder_Alps_EC11E-Switch_Vertical_H20mm')

# RC input filters – rot=90 makes resistors horizontal (series with signal)
add('Device:R', 'R1', '100',   155, 75,  rot=90, fp='Resistor_SMD:R_0603_1608Metric')
add('Device:R', 'R2', '100',   155, 120, rot=90, fp='Resistor_SMD:R_0603_1608Metric')

# Shunt capacitors hang below the filter output node (same x as R left pin)
add('Device:C', 'C1', '100nF', 151, 88,  fp='Capacitor_SMD:C_0603_1608Metric')
add('Device:C', 'C2', '100nF', 151, 133, fp='Capacitor_SMD:C_0603_1608Metric')

# Pico supply decoupling
add('Device:C', 'C3', '10uF',  60, 55, fp='Capacitor_SMD:C_0805_2012Metric')
add('Device:C', 'C4', '100nF', 70, 55, fp='Capacitor_SMD:C_0603_1608Metric')

# INA supply decoupling
add('Device:C', 'C5', '100nF', 175, 60,  fp='Capacitor_SMD:C_0603_1608Metric')
add('Device:C', 'C6', '100nF', 175, 105, fp='Capacitor_SMD:C_0603_1608Metric')

# Connectors
add('Connector_Generic:Conn_01x04', 'J1', 'Shunt_1', 230, 70,
    fp='Connector_PinHeader_2.54mm:PinHeader_1x04_P2.54mm_Vertical')
add('Connector_Generic:Conn_01x04', 'J2', 'Shunt_2', 230, 115,
    fp='Connector_PinHeader_2.54mm:PinHeader_1x04_P2.54mm_Vertical')
add('Connector_Generic:Conn_01x03', 'J3', 'Gate_Drive', 230, 155,
    fp='Connector_PinHeader_2.54mm:PinHeader_1x03_P2.54mm_Vertical')
add('Connector_Generic:Conn_01x05', 'J4', 'SWD_Debug', 25, 145,
    fp='Connector_PinHeader_2.54mm:PinHeader_1x05_P2.54mm_Vertical')

sch.save_as(OUT)
print("Pass 1 complete – symbols placed.")

# ─────────────────────────────────────────────────────────────────────────────
# PASS 2 – Wiring, labels, power symbols
# ─────────────────────────────────────────────────────────────────────────────

sch = load_schematic(OUT)
C   = sch.components

def pin(ref, num):
    """Return (x, y) of a component pin, rounded to 2 dp."""
    pos = sch.get_component_pin_position(ref, num)
    if pos is None:
        raise ValueError(f"Pin {ref}:{num} not found")
    return round(pos.x, 2), round(pos.y, 2)

# Collect all pin positions we need
u1_3v3        = pin('U1', '36')          # 3V3 power output
u1_adc_vref   = pin('U1', '35')          # ADC_VREF
u1_gnd        = pin('U1', '3')           # first GND
u1_agnd       = pin('U1', '33')          # AGND (analog GND)
u1_gpio6      = pin('U1', '9')           # DISABLE
u1_gpio8      = pin('U1', '11')          # ENC_A
u1_gpio9      = pin('U1', '12')          # ENC_B
u1_adc0       = pin('U1', '31')          # GPIO26_ADC0
u1_pwm        = pin('U1', '32')          # GPIO27_ADC1 → gate PWM
u1_adc2       = pin('U1', '34')          # GPIO28_ADC2

u2_out        = pin('U2', '1')           # INA1 output
u2_gnd        = pin('U2', '2')
u2_inp        = pin('U2', '3')           # IN+
u2_inn        = pin('U2', '4')           # IN-
u2_ref        = pin('U2', '5')
u2_vcc        = pin('U2', '6')

u3_out        = pin('U3', '1')
u3_gnd        = pin('U3', '2')
u3_inp        = pin('U3', '3')
u3_inn        = pin('U3', '4')
u3_ref        = pin('U3', '5')
u3_vcc        = pin('U3', '6')

sw1_a         = pin('SW1', 'A')
sw1_b         = pin('SW1', 'B')
sw1_c         = pin('SW1', 'C')

r1_ina        = pin('R1', '1')           # right side (INA input)
r1_adc        = pin('R1', '2')           # left  side (filter output → ADC)
r2_ina        = pin('R2', '1')
r2_adc        = pin('R2', '2')

c1_top        = pin('C1', '1')
c1_bot        = pin('C1', '2')
c2_top        = pin('C2', '1')
c2_bot        = pin('C2', '2')

c3_top        = pin('C3', '1')
c3_bot        = pin('C3', '2')
c4_top        = pin('C4', '1')
c4_bot        = pin('C4', '2')
c5_top        = pin('C5', '1')
c5_bot        = pin('C5', '2')
c6_top        = pin('C6', '1')
c6_bot        = pin('C6', '2')

j1 = {str(n): pin('J1', str(n)) for n in range(1, 5)}
j2 = {str(n): pin('J2', str(n)) for n in range(1, 5)}
j3 = {str(n): pin('J3', str(n)) for n in range(1, 4)}
j4 = {str(n): pin('J4', str(n)) for n in range(1, 6)}

# Additional U1 pins
u1_run    = pin('U1', '30')   # RUN / nRESET
u1_3v3_en = pin('U1', '37')   # 3V3_EN – must be pulled high to enable regulator
u1_gnd38  = pin('U1', '38')   # GND (bottom)
u1_vsys   = pin('U1', '39')   # VSYS power input (5 V supply)
# Passive GND pins distributed around the Pico module
u1_gnd_extra = [pin('U1', str(n)) for n in (8, 13, 18, 23, 28)]

print("Pin positions resolved.")

# ── Power symbols ─────────────────────────────────────────────────────────────
_pwr_n = [0]
def pwr(sym, x, y):
    _pwr_n[0] += 1
    C.add(f'power:{sym}', reference=f'#PWR{_pwr_n[0]:02d}',
          value=sym, position=(x, y))

# +3V3 at each component supply pin (including Pico output and ADC reference)
pwr('+3V3', *u1_3v3)       # Pico 3V3(OUT) drives the rail
pwr('+3V3', *u1_adc_vref)  # ADC_VREF tied to 3.3 V
pwr('+3V3', *u1_3v3_en)    # 3V3_EN pulled high to enable regulator
pwr('+3V3', *u2_vcc)
pwr('+3V3', *u3_vcc)
pwr('+3V3', *c3_top)
pwr('+3V3', *c4_top)
pwr('+3V3', *c5_top)
pwr('+3V3', *c6_top)
pwr('+3V3', *j4['1'])      # J4 pin 1: SWD VCC

# GND at every ground pin
for pt in (u2_gnd, u2_ref, u3_gnd, u3_ref,
           c3_bot, c4_bot, c5_bot, c6_bot,
           c1_bot, c2_bot,
           j1['4'], j2['4'], j3['3'],
           j4['3'],           # J4 pin 3: SWD GND
           u1_gnd38,          # U1 bottom GND
           *u1_gnd_extra):    # U1 distributed GND pins (8,13,18,23,28)
    pwr('GND', *pt)

# Encoder common → GND
pwr('GND', *sw1_c)

# +12V load supply into J1 pin 1
pwr('+12V', *j1['1'])

# +5V supplies VSYS (Pico power input when not using USB)
pwr('+5V', *u1_vsys)

# ── Net labels (long-distance logical connections) ────────────────────────────
nets = [
    # Encoder quadrature to Pico
    ('ENC_A',    *sw1_a),
    ('ENC_A',    *u1_gpio8),
    ('ENC_B',    *sw1_b),
    ('ENC_B',    *u1_gpio9),

    # INA1 output → series R1 → ADC0
    ('ISENSE1',  *u2_out),
    ('ISENSE1',  *r1_ina),   # right pin of R1 (INA side)
    ('ADC0',     *r1_adc),   # left  pin of R1 (filter output)
    ('ADC0',     *u1_adc0),

    # INA2 output → series R2 → ADC2
    ('ISENSE2',  *u3_out),
    ('ISENSE2',  *r2_ina),
    ('ADC2',     *r2_adc),
    ('ADC2',     *u1_adc2),

    # Gate drive (PWM5B) and load enable
    ('GATE_PWM', *u1_pwm),
    ('GATE_PWM', *j3['1']),
    ('DISABLE',  *u1_gpio6),
    ('DISABLE',  *j3['2']),

    # Shunt sense inputs to INA1
    ('SH1_P',    *j1['2']),
    ('SH1_P',    *u2_inp),
    ('SH1_N',    *j1['3']),
    ('SH1_N',    *u2_inn),

    # Shunt sense inputs to INA2
    ('SH2_P',    *j2['2']),
    ('SH2_P',    *u3_inp),
    ('SH2_N',    *j2['3']),
    ('SH2_N',    *u3_inn),

    # SWD debug connector J4 signals
    ('SWDIO',    *j4['2']),   # J4 pin 2 – data (no matching Pico module pin)
    ('SWDCLK',   *j4['4']),   # J4 pin 4 – clock (no matching Pico module pin)
    ('NRESET',   *j4['5']),   # J4 pin 5 – reset
    ('NRESET',   *u1_run),    # U1 pin 30 (RUN) = active-high reset
]

for net, x, y in nets:
    sch.add_label(net, position=(x, y))

# ── Local wires (RC filter junctions) ────────────────────────────────────────
# Vertical stub: R1 ADC-side pin → C1 top (same x column, straight down)
sch.add_wire(r1_adc, c1_top)
# Channel 2
sch.add_wire(r2_adc, c2_top)

# ── Save ─────────────────────────────────────────────────────────────────────
sch.save_as(OUT)
print(f"Pass 2 complete – schematic written to {OUT}")
stats = sch.get_statistics()
print(f"  Components : {stats.get('components', '?')}")
print(f"  Labels     : {stats.get('labels', '?')}")
print(f"  Wires      : {stats.get('wires', '?')}")
