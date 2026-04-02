export interface Theme {
  bg0: string; bg1: string; bg2: string; bg3: string;
  side: string; hover: string;
  input: string;
  brd: string; brdSub: string;
  t0: string; t1: string; t2: string; t3: string;
  ac: string; acS: string; acT: string;
  mp: string; mpB: string;
  md: string; mdB: string;
  ma: string; maB: string;
  mb: string; mbB: string;
  ok: string; okS: string;
  er: string; erS: string;
  shS: string; shL: string;
  wave: string;
  winBtn: string; winClose: string;
}

export const lightTheme: Theme = {
  bg0: '#F9F9FA', bg1: '#FFFFFF', bg2: '#F2F2F4', bg3: '#E8E8EB',
  side: '#F5F5F7', hover: '#EDEDF0',
  input: '#F0F0F3',
  brd: 'rgba(0,0,0,0.06)', brdSub: 'rgba(0,0,0,0.04)',
  t0: '#1A1A1E', t1: '#55555E', t2: '#88888F', t3: '#B0B0B8',
  ac: '#6880A8', acS: 'rgba(104,128,168,0.07)', acT: '#5A72A0',
  mp: '#6880A8', mpB: 'rgba(104,128,168,0.06)',
  md: '#88888F', mdB: 'rgba(136,136,143,0.06)',
  ma: '#8878A0', maB: 'rgba(136,120,160,0.06)',
  mb: '#A08078', mbB: 'rgba(160,128,120,0.06)',
  ok: '#5E9A78', okS: 'rgba(94,154,120,0.07)',
  er: '#A08078', erS: 'rgba(160,128,120,0.07)',
  shS: '0 1px 2px rgba(0,0,0,0.02)', shL: '0 4px 12px rgba(0,0,0,0.04)',
  wave: '#6880A8',
  winBtn: '#888', winClose: '#C44',
};

export const darkTheme: Theme = {
  bg0: '#201E1C', bg1: '#282624', bg2: '#302E2B', bg3: '#3A3836',
  side: '#242220', hover: '#322F2C',
  input: '#2C2A28',
  brd: 'rgba(255,255,255,0.06)', brdSub: 'rgba(255,255,255,0.04)',
  t0: '#E0DDD8', t1: '#9A9690', t2: '#6E6A65', t3: '#4A4742',
  ac: '#B8A07A', acS: 'rgba(184,160,122,0.09)', acT: '#CCBA98',
  mp: '#B8A07A', mpB: 'rgba(184,160,122,0.08)',
  md: '#6E6A65', mdB: 'rgba(110,106,101,0.08)',
  ma: '#9080A0', maB: 'rgba(144,128,160,0.08)',
  mb: '#A07068', mbB: 'rgba(160,112,104,0.08)',
  ok: '#6E9E80', okS: 'rgba(110,158,128,0.08)',
  er: '#A07068', erS: 'rgba(160,112,104,0.08)',
  shS: 'none', shL: '0 4px 12px rgba(0,0,0,0.2)',
  wave: '#B8A07A',
  winBtn: '#6E6A65', winClose: '#A07068',
};

export const MONO = "'JetBrains Mono','Consolas',monospace";
export const SANS = "'DM Sans',-apple-system,'Segoe UI',sans-serif";

export const pingColor = (T: Theme, ms: number) =>
  ms < 50 ? T.ok : ms < 70 ? T.ac : T.er;

export const pingBg = (T: Theme, ms: number) =>
  ms < 50 ? T.okS : ms < 70 ? T.acS : T.erS;

export const modeColor = (T: Theme, m: string) =>
  m === 'proxy' ? T.mp : m === 'direct' ? T.md : m === 'auto' ? T.ma : T.mb;

export const modeBg = (T: Theme, m: string) =>
  m === 'proxy' ? T.mpB : m === 'direct' ? T.mdB : m === 'auto' ? T.maB : T.mbB;
