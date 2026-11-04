/** Browser mode never runs in a Flatpak sandbox. */
import { registerMock } from "@/ipc";
import type { FlatpakStatus } from "@/ipc/flatpak";

registerMock("flatpak_status", (): FlatpakStatus => ({
  inFlatpak: false,
  flatpakId: null,
  enabled: true,
  active: false,
  hostOsVisible: false,
}));
