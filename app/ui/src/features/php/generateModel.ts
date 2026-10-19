/** Pure logic behind the Alt+Insert "Generate" popup and its chooser dialogs. */
import type { FieldTarget, GenerateKind, GenerateTargets, MethodTarget } from "./api";

export interface GenerateMenuItem {
  kind: GenerateKind;
  label: string;
  /** Mnemonic letter index in the label (underlined, typed to pick). */
  mnemonic: number;
  shortcut?: string;
  enabled: boolean;
  /** Group separator before this item. */
  separator?: boolean;
}

const settable = (f: FieldTarget) => !f.isReadonly;

/** Fields offered in the chooser for `kind` (PHPStorm hides fields that already have accessors). */
export function fieldsFor(kind: GenerateKind, t: GenerateTargets): FieldTarget[] {
  switch (kind) {
    case "constructor":
      return t.fields.filter((f) => !f.isStatic && !f.promoted);
    case "getters":
      return t.fields.filter((f) => !f.hasGetter);
    case "setters":
      return t.fields.filter((f) => !f.hasSetter && settable(f));
    case "gettersAndSetters":
      return t.fields.filter((f) => !f.hasGetter || (!f.hasSetter && settable(f)));
    case "toString":
      return t.fields.filter((f) => !f.isStatic);
    default:
      return [];
  }
}

export function methodsFor(kind: GenerateKind, t: GenerateTargets): MethodTarget[] {
  if (kind === "overrideMethods") return t.overrideMethods;
  if (kind === "implementMethods") return t.implementMethods;
  return [];
}

export function generateMenu(t: GenerateTargets): GenerateMenuItem[] {
  const inClass = !!t.className && t.classKind !== "interface";
  return [
    { kind: "constructor", label: "Constructor", mnemonic: 0, enabled: inClass && !t.hasConstructor },
    { kind: "getters", label: "Getters", mnemonic: 0, enabled: inClass && fieldsFor("getters", t).length > 0 },
    { kind: "setters", label: "Setters", mnemonic: 0, enabled: inClass && fieldsFor("setters", t).length > 0 },
    {
      kind: "gettersAndSetters",
      label: "Getters and Setters",
      mnemonic: 12,
      enabled: inClass && fieldsFor("gettersAndSetters", t).length > 0,
    },
    {
      kind: "overrideMethods",
      label: "Override Methods…",
      mnemonic: 0,
      shortcut: "Ctrl+O",
      enabled: inClass && t.overrideMethods.length > 0,
      separator: true,
    },
    {
      kind: "implementMethods",
      label: "Implement Methods…",
      mnemonic: 0,
      shortcut: "Ctrl+I",
      enabled: !!t.className && t.implementMethods.length > 0,
    },
    { kind: "toString", label: "toString()", mnemonic: 0, enabled: inClass && !t.hasToString, separator: true },
    { kind: "phpDoc", label: "PHPDoc Comment", mnemonic: 3, enabled: !!t.phpDocTarget },
  ];
}

/** Title of the chooser dialog (PHPStorm wording). */
export function chooserTitle(kind: GenerateKind): string {
  switch (kind) {
    case "constructor":
      return "Choose Fields to Initialize by Constructor";
    case "getters":
      return "Choose Fields to Generate Getters";
    case "setters":
      return "Choose Fields to Generate Setters";
    case "gettersAndSetters":
      return "Choose Fields to Generate Getters and Setters";
    case "toString":
      return "Choose Fields to Include in toString()";
    case "overrideMethods":
      return "Select Methods to Override";
    case "implementMethods":
      return "Select Methods to Implement";
    default:
      return "Generate";
  }
}

/** Whether the action needs the chooser (PHPDoc applies directly). */
export const needsChooser = (kind: GenerateKind) => kind !== "phpDoc";

export const showsFluentOption = (kind: GenerateKind) => kind === "setters" || kind === "gettersAndSetters";

/** Methods grouped by declaring class/interface, in hierarchy order. */
export function groupByOwner(methods: MethodTarget[]): { owner: string; kind: string; methods: MethodTarget[] }[] {
  const groups = new Map<string, { owner: string; kind: string; methods: MethodTarget[] }>();
  for (const m of methods) {
    const g = groups.get(m.owner) ?? { owner: m.owner, kind: m.ownerKind, methods: [] };
    g.methods.push(m);
    groups.set(m.owner, g);
  }
  return [...groups.values()];
}

/** Clamp a popup of `size` at `pos` into the viewport. */
export function clampToViewport(
  pos: { x: number; y: number },
  size: { width: number; height: number },
  viewport: { width: number; height: number },
  margin = 8,
): { x: number; y: number } {
  return {
    x: Math.max(margin, Math.min(pos.x, viewport.width - size.width - margin)),
    y: Math.max(margin, Math.min(pos.y, viewport.height - size.height - margin)),
  };
}
