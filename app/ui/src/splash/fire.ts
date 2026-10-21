/**
 * The molten fill of the splash wordmark: eases the shown fill towards the real progress
 * (`--p` on the splash element clips the hot layer), shows the glowing front edge and throws
 * small sparks from it on a canvas. With `prefers-reduced-motion` there are no sparks and the
 * fill jumps instead of easing.
 */
export interface Fire {
  /** 0–100 */
  setProgress(percent: number): void;
  /** Throw `n` sparks from the front edge. */
  burst(n: number): void;
  dispose(): void;
}

interface Spark {
  x: number;
  y: number;
  vx: number;
  vy: number;
  life: number;
  decay: number;
  w: number;
}

export function startFire(splash: HTMLElement, word: HTMLElement, canvas: HTMLCanvasElement): Fire {
  const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;
  const ctx = canvas.getContext("2d");
  const front = word.querySelector<HTMLElement>(".front");
  const root = document.documentElement;
  const css = (name: string) => getComputedStyle(root).getPropertyValue(name).trim();
  let w = 0;
  let h = 0;
  const size = () => {
    const r = canvas.getBoundingClientRect();
    const d = Math.min(2, devicePixelRatio || 1);
    w = r.width;
    h = r.height;
    canvas.width = w * d;
    canvas.height = h * d;
    ctx?.setTransform(d, 0, 0, d, 0, 0);
  };
  const resize = new ResizeObserver(size);
  resize.observe(canvas);
  size();

  let progress = 0;
  let shown = 0;
  const sparks: Spark[] = [];

  const frontPoint = () => {
    const s = splash.getBoundingClientRect();
    const r = word.getBoundingClientRect();
    return {
      x: r.left - s.left + (r.width * shown) / 100,
      top: r.top - s.top + r.height * 0.18,
      bottom: r.top - s.top + r.height * 0.82,
    };
  };

  const burst = (n: number) => {
    if (reduce) return;
    const f = frontPoint();
    for (let i = 0; i < n; i++) {
      const y = f.top + Math.random() * (f.bottom - f.top);
      const angle = -Math.PI / 2 + (Math.random() - 0.35) * 1.9;
      const speed = (1.2 + Math.random() * 3.4) * (w / 640);
      sparks.push({
        x: f.x,
        y,
        vx: Math.cos(angle) * speed + 0.8,
        vy: Math.sin(angle) * speed,
        life: 1,
        decay: 0.014 + Math.random() * 0.02,
        w: 0.8 + Math.random() * 1.4,
      });
    }
  };

  let last = performance.now();
  let raf = 0;
  const frame = (now: number) => {
    const dt = Math.min(48, now - last) / 16.67;
    last = now;
    shown = reduce ? progress : shown + (progress - shown) * Math.min(1, 0.12 * dt);
    splash.style.setProperty("--p", `${shown}%`);
    const burning = shown > 0.5 && shown < 99.5;
    front?.style.setProperty("--front", burning ? "1" : "0");
    if (burning && Math.random() < 0.35 * dt) burst(1);
    if (ctx) {
      ctx.clearRect(0, 0, w, h);
      const hot = css("--splash-molten-3");
      const warm = css("--splash-molten-2");
      for (let i = sparks.length - 1; i >= 0; i--) {
        const p = sparks[i];
        p.vy += 0.09 * dt;
        p.x += p.vx * dt;
        p.y += p.vy * dt;
        p.life -= p.decay * dt;
        if (p.life <= 0) {
          sparks.splice(i, 1);
          continue;
        }
        ctx.globalAlpha = p.life;
        ctx.strokeStyle = p.life > 0.55 ? hot : warm;
        ctx.lineWidth = p.w;
        ctx.lineCap = "round";
        ctx.beginPath();
        ctx.moveTo(p.x, p.y);
        ctx.lineTo(p.x - p.vx * 2, p.y - p.vy * 2);
        ctx.stroke();
      }
      ctx.globalAlpha = 1;
    }
    raf = requestAnimationFrame(frame);
  };
  raf = requestAnimationFrame(frame);

  return {
    setProgress(percent) {
      progress = Math.max(0, Math.min(100, percent));
      splash.style.setProperty("--heat", String(progress / 100));
    },
    burst,
    dispose() {
      cancelAnimationFrame(raf);
      resize.disconnect();
    },
  };
}
