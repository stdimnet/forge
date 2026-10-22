/**
 * The editor tab context menu is contributed by a feature (`features/editorTabs`); the
 * shell only asks for it when a tab is right-clicked.
 */
type TabMenuProvider = (x: number, y: number, group: number, id: string) => void;

let provider: TabMenuProvider | undefined;

export function setTabMenuProvider(p: TabMenuProvider): void {
  provider = p;
}

export function openTabMenu(x: number, y: number, group: number, id: string): void {
  provider?.(x, y, group, id);
}
