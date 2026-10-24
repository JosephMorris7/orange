// The startup animation, in three beats:
//   1. the Orange wheel pops up in the middle of the screen and spins, and the name "Orange" fades in under it;
//   2. when the spin ends Orange's window opens (Rust is told with `splash_opened`);
//   3. at that same instant the wheel drops along a curve into the tray icon and dissolves with a small pulse.
// Rust only supplies the target (see splash.rs); everything visible is drawn here.
import { api, call, isTauri } from './api';

interface Plan { target_x: number; target_y: number; width: number; height: number }

const stage = document.getElementById('stage')!;
const DEV = !isTauri();
if (DEV) document.body.classList.add('dev');

const GHOSTS = 6;
const GHOST_LAG = 30; // ms between trail copies
const LOGO = 160;     // css px at scale 1
const ICON = 20;      // css px the logo shrinks to (a tray icon)

const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
// phase boundaries in ms
const T = reduced
  ? { appear: 200, spin: 500, fly: 450, land: 200 }
  : { appear: 450, spin: 1150, fly: 800, land: 380 };
const T_FLY = T.appear + T.spin;   // the spin ends and Orange's window opens
const T_LAND = T_FLY + T.fly;
const T_END = T_LAND + T.land;
const SPIN_DEG = 900;              // total turn before the wheel comes to rest
const ROLL_DEG = 240;              // extra roll while it drops into the tray

const clamp01 = (v: number) => Math.min(1, Math.max(0, v));
const easeOutBack = (p: number) => { const c1 = 1.9, c3 = c1 + 1; return 1 + c3 * Math.pow(p - 1, 3) + c1 * Math.pow(p - 1, 2); };
const easeOutQuart = (p: number) => 1 - Math.pow(1 - p, 4);
const easeInOutSine = (p: number) => -(Math.cos(Math.PI * p) - 1) / 2;
const easeDrop = (p: number) => Math.pow(p, 1.8); // starts gently, then falls faster
const lerp = (a: number, b: number, p: number) => a + (b - a) * p;

interface Pose { x: number; y: number; s: number; rot: number; o: number }

function makePath(w: number, h: number, tx: number, ty: number) {
  const x0 = w / 2, y0 = h / 2;
  // a gentle arc: the control point sits to one side of the straight line, pulled upward
  const mx = (x0 + tx) / 2, my = (y0 + ty) / 2;
  const dx = tx - x0, dy = ty - y0, len = Math.hypot(dx, dy) || 1;
  const nx = -dy / len, ny = dx / len;          // perpendicular
  const side = ny < 0 ? 1 : -1;                  // bulge upward
  const bulge = Math.min(260, len * 0.28);
  const cx = mx + nx * bulge * side, cy = my + ny * bulge * side;
  const at = (p: number): [number, number] => {
    const q = 1 - p;
    return [q * q * x0 + 2 * q * p * cx + p * p * tx, q * q * y0 + 2 * q * p * cy + p * p * ty];
  };
  return { at, x0, y0 };
}

function poseAt(t: number, path: ReturnType<typeof makePath>, endScale: number): Pose {
  const spin = reduced ? 0 : SPIN_DEG;
  const roll = reduced ? 0 : ROLL_DEG;
  if (t < T_FLY) {
    const rot = spin * easeOutQuart(clamp01(t / T_FLY));
    const p = clamp01(t / T.appear);
    return { x: path.x0, y: path.y0, s: t < T.appear ? lerp(0.35, 1, easeOutBack(p)) : 1, rot, o: clamp01(p * 2.2) };
  }
  if (t < T_LAND) {
    const p = easeDrop((t - T_FLY) / T.fly);
    const [x, y] = path.at(p);
    return { x, y, s: lerp(1, endScale, p), rot: spin + roll * p, o: 1 };
  }
  const p = clamp01((t - T_LAND) / T.land);
  const [x, y] = path.at(1);
  return { x, y, s: endScale * (1 - 0.15 * p), rot: spin + roll, o: 1 - easeInOutSine(clamp01((p - 0.35) / 0.65)) };
}

function place(el: HTMLElement, pose: Pose, opacity: number): void {
  el.style.opacity = String(Math.max(0, Math.min(1, opacity)));
  el.style.transform = `translate3d(${pose.x.toFixed(1)}px, ${pose.y.toFixed(1)}px, 0) translate(-50%, -50%) rotate(${pose.rot.toFixed(1)}deg) scale(${pose.s.toFixed(4)})`;
}

function mark(cls: string): HTMLImageElement {
  const img = document.createElement('img');
  img.className = cls;
  img.src = './mark.svg';
  img.alt = '';
  img.draggable = false;
  stage.append(img);
  return img;
}

/** Play once; `onSpinEnd` fires the moment the spin is over and the drop begins. Resolves after the last frame. */
function play(plan: Plan, onSpinEnd: () => void): Promise<void> {
  stage.replaceChildren();
  // the page works in css pixels; the plan is in physical pixels
  const dpr = window.devicePixelRatio || 1;
  const w = plan.width / dpr, h = plan.height / dpr;
  const tx = plan.target_x / dpr, ty = plan.target_y / dpr;
  const path = makePath(w, h, tx, ty);
  const endScale = ICON / LOGO;

  const veil = document.createElement('div');
  veil.className = 'veil';
  veil.style.background = `radial-gradient(circle at ${(w / 2).toFixed(0)}px ${(h / 2).toFixed(0)}px, rgba(16,12,9,.62) 0, rgba(16,12,9,.38) 22%, rgba(16,12,9,0) 52%)`;
  stage.append(veil);
  const ghosts = reduced ? [] : Array.from({ length: GHOSTS }, () => mark('mark ghost'));
  const logo = mark('mark');
  const word = document.createElement('div');
  word.className = 'word';
  word.textContent = 'Orange';
  word.style.left = `${(w / 2).toFixed(1)}px`;
  word.style.top = `${(h / 2 + LOGO / 2 + 30).toFixed(1)}px`;
  stage.append(word);
  const ring = document.createElement('div');
  ring.className = 'ring';
  stage.append(ring);

  return new Promise((resolve) => {
    const start = performance.now();
    let opened = false;
    const frame = (now: number) => {
      const t = now - start;
      if (!opened && t >= T_FLY) { opened = true; onSpinEnd(); }
      const pose = poseAt(Math.min(t, T_END), path, endScale);
      place(logo, pose, pose.o);

      // the name: fades in under the spinning wheel, gone as the wheel leaves
      const inP = clamp01((t - 380) / 420), outP = clamp01((t - T_FLY) / 240);
      word.style.opacity = String(easeInOutSine(inP) * (1 - outP));
      word.style.transform = `translate(-50%, 0) translateY(${((1 - inP) * 14).toFixed(1)}px)`;
      // the dim backdrop that makes the wheel and the name readable on any wallpaper
      veil.style.opacity = String(easeInOutSine(clamp01(t / 500)) * (1 - easeInOutSine(clamp01((t - T_FLY) / 520))));

      // comet trail: lagged copies of the same motion, only while dropping
      ghosts.forEach((g, i) => {
        const tg = t - (i + 1) * GHOST_LAG;
        const flying = tg > T_FLY && tg < T_LAND + 60;
        if (!flying) { g.style.opacity = '0'; return; }
        const gp = poseAt(tg, path, endScale);
        place(g, gp, 0.34 * (1 - i / GHOSTS) * gp.o);
      });

      // arrival pulse
      const rp = clamp01((t - T_LAND) / (T.land * 0.9));
      if (rp > 0 && rp < 1) {
        const rs = lerp(1, 3.2, easeInOutSine(rp));
        ring.style.opacity = String(0.65 * (1 - rp));
        ring.style.transform = `translate3d(${tx.toFixed(1)}px, ${ty.toFixed(1)}px, 0) translate(-50%, -50%) scale(${rs.toFixed(3)})`;
      } else ring.style.opacity = '0';

      if (t < T_END) requestAnimationFrame(frame);
      else resolve();
    };
    requestAnimationFrame(frame);
  });
}

async function main(): Promise<void> {
  if (DEV) {
    // browser preview: aim at the bottom-right corner, replay on click
    const dpr = window.devicePixelRatio || 1;
    const plan: Plan = { target_x: Math.round((innerWidth - 70) * dpr), target_y: Math.round((innerHeight - 24) * dpr), width: Math.round(innerWidth * dpr), height: Math.round(innerHeight * dpr) };
    const run = () => { void play(plan, () => document.body.classList.add('ui-open')); };
    document.body.style.pointerEvents = 'auto';
    document.body.addEventListener('click', run);
    run();
    return;
  }
  try {
    const plan = await call<Plan>('splash_ready');
    await play(plan, () => { void call('splash_opened').catch(() => {}); });
  } catch (e) {
    void api.log(`splash: ${String(e)}`);
  } finally {
    await call('splash_done').catch(() => {});
  }
}
void main();
