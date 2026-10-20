/** Typed client for the Doctrine backend (`app/src-tauri/src/commands/doctrine.rs`). */
import { invoke } from "./index";
import type { SourceLocation } from "./symfony";

export interface DoctrineField {
  name: string;
  /** Doctrine type, or the embeddable class for embedded fields. */
  type: string;
  column?: string;
  length?: number;
  nullable?: boolean;
  unique?: boolean;
  id?: boolean;
  generated?: boolean;
  embedded?: boolean;
  line: number;
}

export type AssociationKind = "manyToOne" | "oneToMany" | "manyToMany" | "oneToOne";

export interface DoctrineAssociation {
  name: string;
  kind: AssociationKind;
  target: string;
  mappedBy?: string;
  inversedBy?: string;
  line: number;
}

export interface DoctrineEntity {
  class: string;
  name: string;
  table?: string;
  repositoryClass?: string;
  path: string;
  line: number;
  source: "attribute" | "annotation" | "xml" | "yaml";
  embeddable?: boolean;
  mappedSuperclass?: boolean;
  fields: DoctrineField[];
  associations: DoctrineAssociation[];
}

export type DqlItemKind = "keyword" | "function" | "entity" | "alias" | "field" | "association" | "embedded";

export interface DqlItem {
  label: string;
  kind: DqlItemKind;
  detail?: string;
  apply?: string;
}

export interface DqlCompletion {
  /** UTF-16 offset where the replaced text starts. */
  from: number;
  items: DqlItem[];
}

export interface DqlDiagnostic {
  start: number;
  end: number;
  message: string;
}

export interface DoctrineGutterMark {
  line: number;
  kind: "entity" | "repository";
  title: string;
  targets: SourceLocation[];
}

export const doctrineApi = {
  entities: (refresh = false) => invoke<DoctrineEntity[]>("doctrine_entities", { refresh }),
  complete: (path: string, content: string, offset: number) =>
    invoke<DqlCompletion | null>("doctrine_complete", { path, content, offset }),
  goto: (path: string, content: string, offset: number) =>
    invoke<SourceLocation[]>("doctrine_goto", { path, content, offset }),
  diagnostics: (path: string, content: string) => invoke<DqlDiagnostic[]>("doctrine_diagnostics", { path, content }),
  gutter: (path: string, content: string) => invoke<DoctrineGutterMark[]>("doctrine_gutter", { path, content }),
};
