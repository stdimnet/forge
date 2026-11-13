import { createMemo, createRoot } from "solid-js";
import { afterEach, describe, expect, it } from "vitest";
import { availableLocales, formatMessage, hasMessage, locale, setLocale, t } from "./index";

afterEach(() => setLocale("en"));

describe("t()", () => {
  it("returns English by default and follows the IDE language", () => {
    expect(t("common.cancel")).toBe("Cancel");
    setLocale("de");
    expect(t("common.cancel")).toBe("Abbrechen");
    expect(document.documentElement.lang).toBe("de");
  });

  it("falls back to English, never to the key", () => {
    setLocale("fr");
    expect(locale()).toBe("fr");
    expect(t("common.cancel")).toBe("Cancel");
  });

  it("formats arguments and plurals in Rust", () => {
    expect(formatMessage("{count, plural, one {# file} other {# files}}", { count: 1 })).toBe("1 file");
    setLocale("de");
    expect(formatMessage("{count, plural, one {# Datei} other {# Dateien}}", { count: 1200 })).toBe("1.200 Dateien");
    expect(hasMessage("common.cancel")).toBe(true);
    expect(hasMessage("no.such.key")).toBe(false);
  });

  it("formats messages with =0 plurals and selects around arguments", () => {
    const del = (count: number, path = "") => t("tools.deployment.deleteFromServerConfirm", { count, path, server: "prod" });
    expect(del(0)).toBe("Delete the project from prod? This cannot be undone.");
    expect(del(1, "src/a.php")).toBe("Delete src/a.php from prod? This cannot be undone.");
    expect(del(3)).toBe("Delete 3 items from prod? This cannot be undone.");
    const remove = (kind: string) => t("tools.remoteHost.deleteConfirm", { kind, path: "/var/www", server: "web" });
    expect(remove("directory")).toBe("Delete the directory /var/www on web with everything in it? This cannot be undone.");
    expect(remove("file")).toBe("Delete /var/www on web? This cannot be undone.");
    expect(t("tools.trust.trustProjectName", { name: "shop" })).toBe("Trust Project 'shop'?");
    setLocale("de");
    expect(del(2)).toBe("2 Elemente von prod löschen? Dies kann nicht rückgängig gemacht werden.");
  });

  it("is reactive to locale changes without a reload", () => {
    createRoot((dispose) => {
      const label = createMemo(() => t("common.cancel"));
      expect(label()).toBe("Cancel");
      setLocale("de");
      expect(label()).toBe("Abbrechen");
      setLocale("en");
      expect(label()).toBe("Cancel");
      dispose();
    });
  });

  it("lists the bundled languages", () => {
    expect(availableLocales().map((l) => l.nativeName)).toEqual(["English", "Deutsch"]);
  });
});
