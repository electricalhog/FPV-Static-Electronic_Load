#!/usr/bin/env python3
"""Regenerate all sim figures from the latest .raw files.
Run from hardware/sim/:  python3 tools/make_figs.py
Datasheet constants below are from hardware/datasheets/ixfn360n10t.pdf (DS100088).
"""
import numpy as np, sys, os
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
sys.path.insert(0, os.path.dirname(__file__))
from ltraw import read_raw

# --- datasheet constants (DS100088) ---------------------------------------
CISS, CRSS_25V, CGD_LO = 36e-9, 330e-12, 1.0e-9   # F
KP, VTO = 213.0, 3.8
GFS_ANCHOR = (60, 160)                             # (A, S)
VPLATEAU = (180, 5.1)                              # gate-charge fig 9
PD25, TJMAX, RTHJC = 830.0, 175.0, 0.18

kw = dict(dpi=130, bbox_inches='tight')

# ---------- Fig 1: transfer validation -------------------------------------
d, _ = read_raw('netlists/dev_transfer.raw')
vgs = d['vgs'].real; Id = -d['I(Vds)'].real
fig, ax = plt.subplots(1, 2, figsize=(11, 4.2))
ax[0].plot(vgs, Id, 'b-', lw=2, label='VDMOS model (Vds=10V)')
ax[0].plot([VPLATEAU[1]], [VPLATEAU[0]], 'r*', ms=16, label='DS Fig9 plateau: 180A@5.1V')
ax[0].axvline(VTO, color='gray', ls=':', label=f'model Vto={VTO}V (spec 2.5-4.5)')
ax[0].set(xlabel='$V_{GS}$ (V)', ylabel='$I_D$ (A)',
          title='IXFN360N10T transfer — model rev B vs DS100088')
ax[0].legend(fontsize=9); ax[0].grid(alpha=.3)
gm = np.gradient(Id, vgs)
ax[1].plot(Id[Id > 1], gm[Id > 1], 'b-', lw=2, label='model $g_m$')
# datasheet Fig 7 (25C) approximate trace points
fig7 = [(20, 85), (40, 120), (60, 150), (100, 185), (160, 215), (240, 240)]
ax[1].plot(*zip(*fig7), 'ko--', ms=5, lw=1, label='DS Fig7 (25°C, approx)')
ax[1].plot([GFS_ANCHOR[0]], [GFS_ANCHOR[1]], 'r*', ms=16, label='DS table: gfs=160S@60A')
ax[1].axvspan(5, 60, color='green', alpha=.08)
ax[1].annotate('operating\nrange/FET', (12, 200), color='green', fontsize=9)
ax[1].set(xlabel='$I_D$ (A)', ylabel='$g_m$ (S)', xlim=(0, 260),
          title='Transconductance — loop gain scales with this')
ax[1].legend(fontsize=9); ax[1].grid(alpha=.3)
fig.savefig('figures/01_fet_model_validation.png', **kw); plt.close(fig)

# ---------- Fig 2: Bode -----------------------------------------------------
z = np.load('out/ac_loop_40A.npz')
f = z['f'].real; T = -z['T']
mag = 20*np.log10(abs(T)); ph = np.unwrap(np.angle(T))*180/np.pi
ph -= round((ph[2]+90)/360)*360
i0 = np.where(np.diff(np.sign(mag)))[0][0]
fc = np.interp(0, [mag[i0+1], mag[i0]], [f[i0+1], f[i0]])
PM = np.interp(np.log10(fc), np.log10(f), ph) + 180
s = np.where(np.diff(np.sign(ph+180)))[0]
f180 = np.interp(-180, [ph[s[0]+1], ph[s[0]]], [f[s[0]+1], f[s[0]]])
GM = -np.interp(np.log10(f180), np.log10(f), mag)
fig, ax = plt.subplots(2, 1, figsize=(9, 6.5), sharex=True)
ax[0].semilogx(f, mag, 'b-', lw=2)
ax[0].axhline(0, color='k', lw=.6); ax[0].axvline(fc, color='r', ls='--', alpha=.6)
ax[0].annotate(f'$f_c$ = {fc/1e3:.1f} kHz', (fc, 4), color='r', fontsize=11)
ax[0].annotate(f'GM = {GM:.0f} dB', (f180, -GM+5), color='purple', fontsize=11)
ax[0].set(ylabel='|T| (dB)',
          title=f'Loop gain — per-FET channel @ 40A, 24V (model rev B)   PM={PM:.0f}°, GM={GM:.0f} dB')
ax[0].grid(which='both', alpha=.3)
ax[1].semilogx(f, ph, 'b-', lw=2)
ax[1].axhline(-180, color='k', lw=.6, ls='--')
ax[1].axvline(fc, color='r', ls='--', alpha=.6)
ax[1].axvline(f180, color='purple', ls=':', alpha=.6)
ax[1].annotate(f'PM = {PM:.0f}°', (fc*1.3, ph[i0]+10), color='r', fontsize=12)
ax[1].set(xlabel='frequency (Hz)', ylabel='phase (°)')
ax[1].grid(which='both', alpha=.3)
fig.savefig('figures/02_loopgain_bode_40A.png', **kw); plt.close(fig)
print(f'fc={fc/1e3:.1f}kHz PM={PM:.1f} GM={GM:.1f}dB')

# ---------- Fig 3: gate-amp requirements ------------------------------------
gm40 = np.sqrt(2*KP*40)
fig, ax = plt.subplots(1, 2, figsize=(11, 4.2))
didt = np.logspace(-1, 2.3, 100)
ig_mA = CISS*(didt/gm40*1e6)*1e3
ax[0].loglog(didt, ig_mA, 'b-', lw=2,
             label=f'$I_g=C_{{iss}}·dV_{{GS}}/dt$  ($C_{{iss}}$=36nF, $g_m$={gm40:.0f}S)')
ax[0].axvline(10, color='gray', ls=':')
ax[0].annotate('10 A/µs\n(fast pulse load)', (11, .05), fontsize=9)
ax[0].set(xlabel='load current slew (A/µs)', ylabel='peak gate current (mA)',
          title='Gate current to slew the load @40A')
ax[0].grid(which='both', alpha=.3); ax[0].legend(fontsize=9)
dvds = np.logspace(-1, 2, 100)
for cgd, lbl, c in [(CRSS_25V, '$C_{gd}$=330pF (VDS=25V, DS table)', 'b'),
                    (CGD_LO, '$C_{gd}$=1.0nF (VDS→0, Fig10)', 'r')]:
    ax[1].loglog(dvds, cgd*(dvds*1e6)*1e3, c, lw=2, label=lbl)
ax[1].axhline(250, color='g', ls='--')
ax[1].annotate('BD139/140 or BUF634A ±250mA', (0.15, 300), color='g', fontsize=9)
ax[1].set(xlabel='DUT $dV_{DS}/dt$ (V/µs)', ylabel='gate current to sink (mA)',
          title='Miller disturbance the buffer must absorb')
ax[1].grid(which='both', alpha=.3); ax[1].legend(fontsize=9)
fig.savefig('figures/03_gate_amp_requirements.png', **kw); plt.close(fig)

# ---------- Fig 4: SOA -------------------------------------------------------
fig, ax = plt.subplots(figsize=(7, 5))
vds = np.logspace(-1, 2, 200)
ax.loglog(vds, np.minimum(360, PD25/vds), 'b-', lw=2, label='DC power limit (830W, Tc=25°C)')
pd75 = PD25*(TJMAX-75)/(TJMAX-25)
ax.loglog(vds, np.minimum(360, pd75/vds), 'b--', lw=1.5, label=f'derated Tc=75°C ({pd75:.0f}W)')
ax.loglog(vds, np.minimum(360, 300/vds), 'r-.', lw=1.5, label='design line 300W/FET (no DS SOA data!)')
ax.plot(24, 50, 'go', ms=10); ax.plot(60, 25, 'mo', ms=10)
ax.plot([], [], 'go', label='3-FET: 150A@24V → 1.2kW/FET (VIOLATES)')
ax.plot([], [], 'mo', label='3-FET: 75A@60V → 500W/FET')
ax.axvline(100, color='k', ls=':', lw=1); ax.annotate('$V_{DSS}$', (75, 1.5), fontsize=9)
ax.set(xlabel='$V_{DS}$ (V)', ylabel='$I_D$ per FET (A)',
       title='IXFN360N10T linear-mode operating region (DS100088 has no SOA curve)',
       xlim=(0.5, 150), ylim=(1, 500))
ax.grid(which='both', alpha=.3); ax.legend(fontsize=8.5, loc='lower left')
fig.savefig('figures/04_soa_preliminary.png', **kw); plt.close(fig)
print('figures regenerated.')
