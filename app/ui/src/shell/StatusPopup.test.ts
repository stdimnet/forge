import { describe, expect, it } from "vitest";
import { placeStatusPopup } from "./StatusPopup";

const viewport = { width: 1440, height: 900 };
/** A status bar widget at the bottom of the window. */
const widget = (left: number, width = 180) => ({ left, right: left + width, top: 877, bottom: 897 });

describe("placeStatusPopup", () => {
  it("opens above the widget, its bottom 6 px above the widget", () => {
    const p = placeStatusPopup(widget(300), { width: 340, height: 190 }, viewport);
    expect(p.above).toBe(true);
    expect(p.top + 190).toBe(877 - 6);
    expect(p.left).toBe(300);
    expect(p.maxHeight).toBe(877 - 12);
  });

  it("aligns with the widget's right edge on request", () => {
    const p = placeStatusPopup(widget(300), { width: 340, height: 190 }, viewport, "end");
    expect(p.left).toBe(480 - 340);
  });

  it("shifts sideways to stay inside the window", () => {
    expect(placeStatusPopup(widget(1300), { width: 340, height: 190 }, viewport).left).toBe(1440 - 340 - 6);
    expect(placeStatusPopup(widget(10), { width: 340, height: 190 }, viewport, "end").left).toBe(6);
  });

  it("limits its height to the room above in a low window", () => {
    const low = { width: 800, height: 150 };
    const p = placeStatusPopup({ left: 10, right: 100, top: 127, bottom: 147 }, { width: 340, height: 190 }, low);
    expect(p.above).toBe(true);
    expect(p.maxHeight).toBe(127 - 12);
    expect(p.top).toBe(6);
  });

  it("flips below a widget near the top when there is more room there", () => {
    const p = placeStatusPopup({ left: 10, right: 100, top: 20, bottom: 40 }, { width: 340, height: 190 }, viewport);
    expect(p.above).toBe(false);
    expect(p.top).toBe(46);
    expect(p.maxHeight).toBe(900 - 40 - 12);
  });
});
