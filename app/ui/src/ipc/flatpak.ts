/**
 * Flatpak mode (Settings | Tools | Flatpak). Rust side: `app/src-tauri/src/commands/flatpak.rs`
 * and `crates/forge-hostspawn`.
 */
import { invoke } from "./index";

export interface FlatpakStatus {
  inFlatpak: boolean;
  flatpakId?: string | null;
  /** The setting "Run tools on the host (Flatpak)". */
  enabled: boolean;
  /** Host tools run through `flatpak-spawn --host` right now. */
  active: boolean;
  /** The host's system folders are visible under `/run/host`. */
  hostOsVisible: boolean;
}

export const flatpakApi = {
  status: () => invoke<FlatpakStatus>("flatpak_status"),
};
