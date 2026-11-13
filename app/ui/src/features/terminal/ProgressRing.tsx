/**
 * Progress reported by a terminal program (OSC 9;4) as a small ring: filled clockwise for a
 * percentage, a spinning arc while indeterminate. Error and paused use the theme's danger
 * and warning colors.
 */
import type { Progress } from "./osc";
import { progressLabel } from "./osc";

export function ProgressRing(props: { progress: Progress; size?: number }) {
  const size = () => props.size ?? 14;
  const indeterminate = () => props.progress.state === "indeterminate";
  // Error / paused without a percentage still show a full ring so the color is visible.
  const value = () =>
    props.progress.state !== "normal" && props.progress.percent === 0 ? 100 : Math.max(props.progress.percent, 2);
  return (
    <svg
      class="term-progress"
      classList={{ [`term-progress-${props.progress.state}`]: true }}
      width={size()}
      height={size()}
      viewBox="0 0 16 16"
      role="progressbar"
      aria-label={progressLabel(props.progress)}
      aria-valuenow={indeterminate() ? undefined : props.progress.percent}
      aria-valuemin={0}
      aria-valuemax={100}
      data-progress-state={props.progress.state}
    >
      <circle class="term-progress-track" cx="8" cy="8" r="6" pathLength="100" />
      <circle
        class="term-progress-value"
        cx="8"
        cy="8"
        r="6"
        pathLength="100"
        stroke-dasharray={indeterminate() ? "28 72" : `${value()} 100`}
        transform="rotate(-90 8 8)"
      />
    </svg>
  );
}
