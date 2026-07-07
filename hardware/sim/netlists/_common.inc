* ===========================================================================
*  Shared models + one per-FET linear-load channel, as a subcircuit.
*  Nodes:  VSET (setpoint in)  DUT (drain supply node)  VCC (+15)
*          GATE SRC (FET gate/source, exposed for probing)
*          EA GDRV  (error-amp out / buffer-in -- the loop-break points)
*          FB (current-sense output)
* ===========================================================================
.include ../models/ixfn360n10t.lib

* ---- design parameters (edit to retune the compensator) -------------------
.param Rs=2m          ; Ohmite TGHG 2 mOhm SOT-227 shunt (shares FET heatsink)
.param Kcs=20         ; INA185A1 = 20 V/V (50A*2m*20 = 2V, fits 3.3V ADC/DAC)
.param fsns=350k      ; INA185A1 bandwidth at gain 20 (datasheet ~350 kHz)
.param Rin=10k        ; error-amp input/feedback resistor
.param Cf=1.8n        ; integrator cap -> sets crossover
.param Rfz=1m         ; series zero R with Cf (~0 = pure integrator; >0 = PI/type-2)
.param Rgext=4.7      ; external gate stopper resistor
.param Gbw=10meg      ; error op-amp gain-bandwidth
.param Aol=1e5        ; error op-amp DC open-loop gain

* ---- behavioural op-amp macromodel (2-pole, output R, rail clamp) ---------
.subckt OPAMP out inp inn vcc vee
Gm  0 n1 inp inn 1m
Rp  n1 0 {Aol/1m}
Cp  n1 0 {1m/(2*3.14159265*Gbw)}     ; dominant pole -> GBW
Rp2 n1 n2 100
Cp2 n2 0 {1/(2*3.14159265*100*30meg)}; 2nd pole ~30MHz (excess phase)
Eb  o2 0 n2 0 1
Ro  out o2 50
* soft rail clamp
D1 out vcc Dclamp
D2 vee out Dclamp
.model Dclamp D(Ron=10 Roff=1meg Vfwd=0.2)
.ends OPAMP

* ---- gate-drive class-AB follower BJTs (medium power ~1.5A) ----------------
.model QNPN NPN(Is=1e-14 Bf=120 Vaf=100 Rb=2 Rc=0.3 Cje=250p Cjc=90p Tf=8n Ikf=3)
.model QPNP PNP(Is=1e-14 Bf=120 Vaf=100 Rb=2 Rc=0.3 Cje=300p Cjc=110p Tf=12n Ikf=3)
.model Dbias D(Is=1e-14 N=1)

* ===========================================================================
.subckt CHANNEL VSET DUT VCC GATE SRC EA GDRV FB
* ---- error amplifier: inverting integrator, non-inv = setpoint ----------
Xea EA VSET ein VCC 0 OPAMP
Rina FB ein {Rin}
Rzc  ea_c ein {Rfz}
Cint EA ea_c {Cf}
* ---- custom analog gate buffer: class-AB push-pull emitter follower ------
*      (this is the "amplifier between feedback amp and FETs")
Iup  VCC nb {3m}
Dpu  nb  GDRV Dbias
Dpd  GDRV pb  Dbias
Idn  pb  0   {3m}
Q1   VCC nb  gbuf QNPN          ; sources gate current (charge Ciss)
Q2   0   pb  gbuf QPNP          ; sinks gate current  (discharge Ciss)
Rgs  gbuf GATE {Rgext}
* ---- power MOSFET (linear/saturation mode) ------------------------------
M1   DUT GATE SRC SRC IXFN360N10T
Rsh  SRC 0 {Rs}                 ; source current shunt
* ---- current-sense amplifier (gain Kcs, 1-pole bandwidth fsns) -----------
Ecs  ncs 0 SRC 0 {Kcs}
Rcs  ncs FB {1k}
Ccs  FB 0 {1/(2*3.14159265*1k*fsns)}
.ends CHANNEL
