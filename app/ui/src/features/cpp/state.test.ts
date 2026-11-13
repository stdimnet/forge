import { describe, expect, it } from "vitest";
import { cmakeFailureDetail } from "./state";

describe("cmakeFailureDetail", () => {
  it("counts the CMake errors with a plural", () => {
    expect(cmakeFailureDetail(0)).toBe("See the CMake tool window");
    expect(cmakeFailureDetail(1)).toBe("1 error: see the CMake tool window");
    expect(cmakeFailureDetail(4)).toBe("4 errors: see the CMake tool window");
  });
});
