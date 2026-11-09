import { describe, expect, it } from "vitest";
import { parseSettingNumber } from "./SchemaPage";

describe("parseSettingNumber", () => {
  it("parses numbers and reports text that is not one", () => {
    expect(parseSettingNumber("42", "int")).toBe(42);
    expect(parseSettingNumber("1.5", "number")).toBe(1.5);
    expect(parseSettingNumber("abc", "int")).toBe("Not a number");
    expect(parseSettingNumber("", "number")).toBe("Not a number");
  });
});
