/**
 * The editor tab context menu is contributed by a feature (`features/editorTabs`); the
 * shell only asks for it when a tab is right-clicked. Likewise a feature decides what a tab
 * dragged out of the window does (`features/windows`: open it in a new window).
 */
type TabMenuProvider = (x: number, y: number, group: number, id: string) => void;

let provider: TabMenuProvider | undefined;

export function setTabMenuProvider(p: TabMenuProvider): void {
  provider = p;
}

export function openTabMenu(x: number, y: number, group: number, id: string): void {
  provider?.(x, y, group, id);
}

type TabDragOutHandler = (group: number, id: string) => void;
let dragOut: TabDragOutHandler | undefined;

/** What happens when a tab is dragged out of the window (a feature opens it in a new one). */
export function setTabDragOutHandler(h: TabDragOutHandler): void {
  dragOut = h;
}

export const canDragTabsOut = (): boolean => !!dragOut;

export function tabDraggedOut(group: number, id: string): void {
  dragOut?.(group, id);
}
