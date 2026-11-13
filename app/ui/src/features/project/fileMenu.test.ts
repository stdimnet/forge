import { describe, expect, it } from "vitest";
import { validateName, validateNewPath } from "./fileMenu";

describe("validateName", () => {
  it("accepts plain names and rejects dot segments and backslashes", () => {
    expect(validateName("Order.php")).toBeNull();
    expect(validateName(".env.local")).toBeNull();
    expect(validateName("..")).not.toBeNull();
    expect(validateName("a\\b")).not.toBeNull();
  });
});

describe("validateNewPath", () => {
  it("allows nested relative paths", () => {
    expect(validateNewPath("app/Models/Order.php")).toBeNull();
  });

  it("rejects absolute paths, parent segments and empty segments", () => {
    expect(validateNewPath("/etc/passwd")).not.toBeNull();
    expect(validateNewPath("../outside.php")).not.toBeNull();
    expect(validateNewPath("a//b.php")).not.toBeNull();
    expect(validateNewPath("a/")).not.toBeNull();
  });
});
