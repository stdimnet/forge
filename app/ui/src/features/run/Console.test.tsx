/** Console file and URL links work from the keyboard (#217). */
import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import type { ConsoleLine } from "@/ipc/run";
import { Console } from "./Console";

const lines: ConsoleLine[] = [
  {
    segments: [
      { text: "Error in ", style: {} },
      { text: "app/Models/User.php:12", style: {}, link: { target: "app/Models/User.php", line: 12 } },
      { text: " see ", style: {} },
      { text: "https://example.test/docs", style: {}, link: { url: true, target: "https://example.test/docs" } },
    ],
  } as ConsoleLine,
];

let dispose: (() => void) | undefined;
afterEach(() => {
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

function mount(onInput?: (text: string) => void) {
  dispose = render(() => <Console lines={() => lines} version={() => 1} softWrap={false} scrollToEnd={false} onInput={onInput} />, document.body);
  return [...document.querySelectorAll<HTMLAnchorElement>("a.run-link")];
}

describe("Console links", () => {
  it("are focusable links: a URL keeps its address, a file link gets '#'", () => {
    const [file, url] = mount();
    expect(file.getAttribute("href")).toBe("#");
    expect(url.getAttribute("href")).toBe("https://example.test/docs");
    expect(file.tabIndex).toBe(0);
  });

  it("open through Forge instead of navigating the window", () => {
    const open = vi.spyOn(window, "open").mockReturnValue(null);
    const [, url] = mount();
    const click = new MouseEvent("click", { bubbles: true, cancelable: true });
    url.dispatchEvent(click);
    expect(click.defaultPrevented).toBe(true);
    expect(open).toHaveBeenCalledWith("https://example.test/docs", "_blank", "noopener");
  });

  it("keep Enter for themselves while the program reads input", () => {
    const onInput = vi.fn();
    const [file] = mount(onInput);
    const enter = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true });
    file.dispatchEvent(enter);
    expect(onInput).not.toHaveBeenCalled();
    expect(enter.defaultPrevented).toBe(false);
    // Typing into the console itself still goes to the program.
    document.querySelector<HTMLElement>(".run-console")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
    expect(onInput).toHaveBeenCalledWith("\n");
  });
});
