import { describe, expect, it } from "vitest";
import { styleOf, tokenize } from "@/features/languages/testkit";
import { gherkin } from "./gherkin";

const FEATURE = `@shop @smoke
Feature: Checkout
  In order to buy products
  # a comment
  Scenario Outline: Buying <count> products
    Given there is a "Lightsaber", which costs £5
    When I add <count> products
    Then the total should be 10
    """
    Given inside a doc string
    """

    Examples:
      | count | total |
      | 2     | £10   |`;

describe("gherkin syntax", () => {
  const lines = tokenize(gherkin, FEATURE);

  it("colors tags, headings, descriptions and comments", () => {
    expect(lines[0]).toEqual([
      ["@shop", "meta"],
      ["@smoke", "meta"],
    ]);
    expect(styleOf(lines[1], "Feature")).toBe("keyword");
    expect(styleOf(lines[1], " Checkout")).toBe("heading");
    expect(lines[2]).toEqual([["In order to buy products", "comment"]]);
    expect(lines[3]).toEqual([["# a comment", "comment"]]);
    expect(styleOf(lines[4], "Scenario Outline")).toBe("keyword");
    expect(styleOf(lines[4], "<count>")).toBe("variable");
  });

  it("colors steps with their arguments", () => {
    expect(styleOf(lines[5], "Given")).toBe("keyword");
    expect(styleOf(lines[5], '"Lightsaber"')).toBe("string");
    expect(styleOf(lines[6], "<count>")).toBe("variable");
    expect(styleOf(lines[7], "10")).toBe("number");
  });

  it("keeps doc strings and tables apart", () => {
    expect(lines[9]).toEqual([["    Given inside a doc string", "string"]]);
    expect(styleOf(lines[13], "count")).toBe("property");
    expect(styleOf(lines[14], "2")).toBe("number");
    expect(styleOf(lines[14], "£10")).toBe("string");
    expect(styleOf(lines[14], "|")).toBe("punct");
  });
});
