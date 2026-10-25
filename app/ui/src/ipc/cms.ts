/** Typed client for the Drupal / Joomla / Shopware backend (`app/src-tauri/src/commands/cms.rs`). */
import { invoke } from "./index";

export interface CmsLocation {
  path: string;
  line: number;
  column?: number;
}

export interface CmsInfo {
  id: "drupal" | "joomla" | "shopware";
  name: string;
  version?: string;
  kind: "site" | "extension";
  extension?: string;
  /** `vendor/bin/drush`, `drush` or `bin/console`. */
  console?: string;
}

export interface DrupalHookCompletion {
  /** `acme_form_alter`. */
  label: string;
  /** `hook_form_alter`. */
  hook: string;
  params: string;
  summary: string;
}

export interface DrupalCompletionList {
  /** UTF-16 offset where the function name starts. */
  from: number;
  items: DrupalHookCompletion[];
}

export interface DrupalHookDocs {
  hook: string;
  /** The `hook_…` function in the project's `*.api.php` files. */
  location?: CmsLocation;
  url: string;
}

export interface DrupalGutterMark {
  line: number;
  title: string;
  docs: DrupalHookDocs;
}

export const CMS_PLUGIN_ID = "forge.cms";

export const cmsApi = {
  detect: () => invoke<CmsInfo[]>("cms_detect"),
  refresh: () => invoke<CmsInfo[]>("cms_refresh"),
  complete: (path: string, content: string, offset: number) =>
    invoke<DrupalCompletionList | null>("cms_complete", { path, content, offset }),
  goto: (path: string, content: string, offset: number) => invoke<DrupalHookDocs | null>("cms_goto", { path, content, offset }),
  gutter: (path: string, content: string) => invoke<DrupalGutterMark[]>("cms_gutter", { path, content }),
};
