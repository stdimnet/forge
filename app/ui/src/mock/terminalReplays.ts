/**
 * Recorded terminal output for browser mode: escape sequence streams that the fake shell
 * replays, so screenshots and manual checks can show what a TUI looks like in Forge's
 * terminal. The "claude" stream imitates a Claude Code session (it is made up, nothing runs):
 * box drawing, truecolor, a braille spinner, wide emoji, powerline / Nerd Font glyphs,
 * underline styles and colors, an OSC 8 link, an OSC 0 title, synchronized output and
 * OSC 9;4 progress from indeterminate to 60 %.
 */

export interface ReplayFrame {
  /** Milliseconds after the previous frame. */
  delay: number;
  data: string;
}

export type ReplayName = "claude" | "claude-thinking" | "claude-error";

export const REPLAYS: ReplayName[] = ["claude", "claude-thinking", "claude-error"];

const ESC = "\x1b";
const BEL = "\x07";
const ST = `${ESC}\\`;
const R = `${ESC}[0m`;
const fg = (r: number, g: number, b: number) => `${ESC}[38;2;${r};${g};${b}m`;
const bg = (r: number, g: number, b: number) => `${ESC}[48;2;${r};${g};${b}m`;
const bold = `${ESC}[1m`;
const dim = `${ESC}[2m`;
const italic = `${ESC}[3m`;
const strike = `${ESC}[9m`;
const overline = `${ESC}[53m`;
/** Curly underline (SGR 4:3) in an RGB underline color (SGR 58). */
const curly = (r: number, g: number, b: number) => `${ESC}[4:3m${ESC}[58:2::${r}:${g}:${b}m`;
const dotted = `${ESC}[4:4m`;

const title = (t: string) => `${ESC}]0;${t}${BEL}`;
/** OSC 9;4 progress: 0 remove, 1 normal, 2 error, 3 indeterminate, 4 paused. */
export const progress = (state: 0 | 1 | 2 | 3 | 4, percent?: number) =>
  `${ESC}]9;4;${state}${percent === undefined ? "" : `;${percent}`}${BEL}`;
const link = (url: string, text: string) => `${ESC}]8;;${url}${ST}${text}${ESC}]8;;${ST}`;
const sync = (s: string) => `${ESC}[?2026h${s}${ESC}[?2026l`;

const CLAUDE = fg(215, 119, 87);
const GRAY = fg(136, 136, 136);
const GREEN = fg(78, 186, 101);
const RED = fg(255, 107, 128);
const YELLOW = fg(229, 180, 73);

/** Terminal cells a string takes (escape sequences stripped; emoji and CJK are wide). */
export function cellWidth(s: string): number {
  const plain = s.replace(/\x1b\][^\x07\x1b]*(\x07|\x1b\\)/g, "").replace(/\x1b\[[0-9;:?]*[A-Za-z]/g, "");
  let w = 0;
  for (const ch of plain) {
    const cp = ch.codePointAt(0)!;
    if (cp === 0xfe0f || cp === 0x200d) continue;
    w += cp >= 0x1f000 || (cp >= 0x1100 && cp <= 0x115f) || (cp >= 0x2e80 && cp <= 0xa4cf) || (cp >= 0xac00 && cp <= 0xd7a3) ? 2 : 1;
  }
  return w;
}

const padTo = (s: string, width: number) => s + " ".repeat(Math.max(0, width - cellWidth(s)));

function box(lines: string[], width: number, color = CLAUDE): string[] {
  const inner = width - 2;
  return [
    `${color}╭${"─".repeat(inner)}╮${R}`,
    ...lines.map((l) => `${color}│${R}${padTo(` ${l}`, inner)}${color}│${R}`),
    `${color}╰${"─".repeat(inner)}╯${R}`,
  ];
}

const BRAILLE = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/** A 10-cell bar from block elements, e.g. "██████░░░░". */
const bar = (pct: number) => {
  const full = Math.round(pct / 10);
  return "█".repeat(full) + "░".repeat(10 - full);
};

/** Custom status line in the style of a powerline prompt (Nerd Font glyphs + emoji). */
function statusLine(pct: number, state: "ok" | "error"): string {
  const seg = (b: [number, number, number], f: [number, number, number], text: string, next?: [number, number, number]) =>
    `${bg(...b)}${fg(...f)} ${text} ${next ? bg(...next) : R}${fg(...b)}${R}`;
  const orange: [number, number, number] = [215, 119, 87];
  const slate: [number, number, number] = [58, 64, 80];
  const teal: [number, number, number] = [42, 96, 104];
  const dark: [number, number, number] = [20, 20, 24];
  const light: [number, number, number] = [235, 235, 240];
  const barColor = state === "error" ? RED : GREEN;
  return (
    seg(orange, dark, `${bold}✻ Opus${R}${bg(...orange)}${fg(...dark)}`, slate) +
    seg(slate, light, " shop", teal) +
    seg(teal, light, " feature/order-totals") +
    `  ${barColor}${bar(pct)}${R} ${pct}%  ${GRAY}ctx${R}  💰 $0.42  ⏱ 1m 12s`
  );
}

const WIDTH = 72;

function inputBox(): string[] {
  return box([`${GRAY}>${R} ${dim}Try "write a test for OrderController"${R}`], WIDTH, GRAY);
}

/** Bottom of the screen that Claude Code redraws on every update. */
function live(spinner: string, pct: number, state: "ok" | "error"): string[] {
  return ["", spinner, ...inputBox(), `  ${GRAY}⏵⏵ accept edits on ${dim}(shift+tab to cycle)${R}`, ` ${statusLine(pct, state)}`];
}

/**
 * Replace the previously drawn live region (`prev` lines, cursor at its end) with the
 * `permanent` lines, which scroll up like normal output, and a new live region.
 */
function redraw(prev: number, permanent: string[], block: string[]): string {
  const up = prev > 1 ? `${ESC}[${prev - 1}A` : "";
  return `\r${up}${ESC}[J${permanent.map((l) => `${l}\r\n`).join("")}${block.join("\r\n")}`;
}

const DIFF_WIDTH = 64;
const diffLine = (n: number, sign: "+" | "-", code: string) => {
  const color = sign === "+" ? bg(28, 72, 44) : bg(92, 32, 38);
  return `      ${color}${fg(220, 220, 228)}${padTo(` ${n} ${sign}   ${code}`, DIFF_WIDTH)}${R}`;
};

/** Build a replay. All streams start like the same Claude Code session. */
export function replayFrames(name: ReplayName, cwd = "/home/dev/Projects/shop"): ReplayFrame[] {
  const frames: ReplayFrame[] = [];
  let liveLines = 0;
  const push = (delay: number, data: string) => frames.push({ delay, data });
  /** Print permanent lines above the live region and redraw it. */
  const emit = (delay: number, permanent: string[], block: string[]) => {
    push(delay, sync(redraw(liveLines, permanent, block)));
    liveLines = block.length;
  };

  push(
    0,
    `${ESC}[2J${ESC}[3J${ESC}[H` +
      title("✳ Claude Code") +
      box(
        [
          `${CLAUDE}✻${R} Welcome to ${bold}Claude Code${R}!`,
          "",
          `  ${GRAY}${italic}/help for help, /status for your current setup${R}`,
          "",
          `  ${GRAY}cwd: ${cwd}${R}`,
        ],
        WIDTH - 14,
      ).join("\r\n") +
      "\r\n\r\n",
  );
  liveLines = 0;
  emit(250, [`${GRAY}>${R} Add a total() method to Order and cover it with a test`, ""], []);

  // Thinking: indeterminate progress and a braille spinner.
  push(150, title("✳ Add Order total") + progress(3));
  const thinkingSteps = name === "claude-thinking" ? 30 : 8;
  for (let i = 0; i < thinkingSteps; i++) {
    emit(90, [], live(`${CLAUDE}${BRAILLE[i % BRAILLE.length]}${R} ${CLAUDE}Thinking…${R} ${GRAY}(${i < 5 ? 2 : 3}s · ${italic}esc to interrupt${R}${GRAY})${R}`, 0, "ok"));
  }
  if (name === "claude-thinking") return frames;

  const tools = [
    `${ESC}[39m●${R} I'll add ${bold}Order::total()${R} and a feature test for it.`,
    "",
    `${GREEN}●${R} ${bold}Read${R}(app/Models/Order.php)`,
    `  ${GRAY}⎿  Read ${bold}48${R}${GRAY} lines (ctrl+r to expand)${R}`,
    "",
    `${GREEN}●${R} ${bold}Update${R}(app/Models/Order.php)`,
    `  ${GRAY}⎿  Updated ${R}${bold}app/Models/Order.php${R}${GRAY} with ${R}${bold}4${R}${GRAY} additions and ${R}${bold}1${R}${GRAY} removal${R}`,
    diffLine(30, "-", `${strike}// TODO: compute the order total${ESC}[29m`),
    diffLine(30, "+", "public function total(): int"),
    diffLine(31, "+", "{"),
    diffLine(32, "+", "    return $this->items->sum(fn ($i) => $i->total());"),
    diffLine(33, "+", "}"),
    `  ${GRAY}⎿  ${R}${YELLOW}⚠${R} ${curly(229, 180, 73)}$i->total()${R} returns cents; see ${link("https://laravel.com/docs/12.x/eloquent-collections#method-sum", `${dotted}Eloquent: sum()${R}`)}`,
    "",
    `${GREEN}●${R} ${bold}Bash${R}(php artisan test --filter=OrderTotalTest)`,
  ];
  emit(200, tools, live(`${CLAUDE}${BRAILLE[0]}${R} ${CLAUDE}Running tests…${R}`, 0, "ok"));

  // Running the tests: determinate progress from 0 to 60 %.
  for (let pct = 0, i = 0; pct <= 60; pct += 5, i++) {
    push(0, progress(1, pct));
    emit(
      140,
      [],
      live(
        `${CLAUDE}${BRAILLE[i % BRAILLE.length]}${R} ${CLAUDE}Running tests…${R} ${GRAY}(${4 + Math.floor(i / 3)}s · ↓ ${(0.8 + i * 0.05).toFixed(1)}k tokens · ${italic}esc to interrupt${R}${GRAY})${R}`,
        pct,
        "ok",
      ),
    );
  }
  if (name === "claude") {
    emit(
      120,
      [
        `  ${GRAY}⎿  ${R}${overline}${bold} PASS ${R} Tests\\Feature\\OrderTotalTest ${GRAY}(3 tests, 0.21s)${R}`,
        `     ${GREEN}✓${R} sums item totals  ${GREEN}✓${R} empty order is 0  ${GREEN}✓${R} rounds cents 🧾`,
      ],
      live(`${CLAUDE}${BRAILLE[3]}${R} ${CLAUDE}Writing summary…${R} ${GRAY}(9s · ${italic}esc to interrupt${R}${GRAY})${R}`, 60, "ok"),
    );
    return frames;
  }

  // claude-error: the tests fail and the program reports an error state.
  push(0, progress(2, 60));
  emit(
    150,
    [
      `  ${GRAY}⎿  ${R}${bg(170, 40, 50)}${fg(255, 255, 255)}${bold} FAIL ${R} Tests\\Feature\\OrderTotalTest`,
      `     ${RED}✗${R} rounds cents ${GRAY}— Failed asserting that 1999 is identical to 2000.${R}`,
    ],
    live(`${RED}✗${R} ${RED}Tests failed${R} ${GRAY}(9s · fixing the rounding)${R}`, 60, "error"),
  );
  return frames;
}

/** Sequence a TUI writes when it exits: clear its progress and restore the title. */
export const replayExit = () => `${progress(0)}${title("")}\r\n`;
