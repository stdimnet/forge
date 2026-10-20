/**
 * PHPStorm's default keymaps as data: "Windows/Linux" ($default) and "macOS".
 *
 * Action ids are PHPStorm's own and double as Forge command ids, so a feature implements
 * e.g. "Go to Class" simply by `registerCommand({ id: "GotoClass", ... })`. Actions marked
 * `native` (clipboard, caret movement, typing) are handled by the browser/CodeMirror and are
 * listed only so they show up in Find Action and the keymap settings.
 */

export type KeymapId = "$default" | "macOS";

export const KEYMAPS: { id: KeymapId; title: string }[] = [
  { id: "$default", title: "PHPStorm (Windows/Linux)" },
  { id: "macOS", title: "PHPStorm (macOS)" },
];

/** Where an action may fire: "editor" actions only while a code editor has focus. */
export type ActionContext = "any" | "editor" | "toolwindow";

export interface ActionDef {
  id: string;
  title: string;
  /** Tree path in the keymap settings, e.g. "Main Menu/Navigate". */
  group: string;
  context: ActionContext;
  native: boolean;
  description?: string;
}

const G = {
  file: "Main Menu/File",
  edit: "Main Menu/Edit",
  find: "Main Menu/Edit/Find",
  view: "Main Menu/View",
  toolWindows: "Main Menu/View/Tool Windows",
  navigate: "Main Menu/Navigate",
  code: "Main Menu/Code",
  folding: "Main Menu/Code/Folding",
  refactor: "Main Menu/Refactor",
  run: "Main Menu/Run",
  git: "Main Menu/Git",
  window: "Main Menu/Window",
  editorTabs: "Main Menu/Window/Editor Tabs",
  bookmarks: "Main Menu/Navigate/Bookmarks",
  editor: "Editor Actions",
  debugger: "Debugger Actions",
  other: "Other",
} as const;

export const ACTION_GROUP_ORDER = [
  "Main Menu",
  "Editor Actions",
  "Debugger Actions",
  "Other",
  "Plugins",
];

interface Opts {
  context?: ActionContext;
  native?: boolean;
  description?: string;
}
const E: Opts = { context: "editor" };
const N: Opts = { native: true };
const EN: Opts = { context: "editor", native: true };
const TW: Opts = { context: "toolwindow" };

type Row = [id: string, title: string, group: string, win: string, mac: string, opts?: Opts];

/* prettier-ignore */
const ROWS: Row[] = [
  // ---- General / search
  ["SearchEverywhere", "Search Everywhere", G.navigate, "Double Shift", "Double Shift"],
  ["GotoAction", "Find Action…", "Main Menu/Help", "Ctrl+Shift+A", "Cmd+Shift+A"],
  ["ShowIntentionActions", "Show Context Actions", G.code, "Alt+Enter", "Alt+Enter"],
  ["ShowSettings", "Settings…", G.file, "Ctrl+Alt+S", "Cmd+,"],
  ["ShowProjectStructureSettings", "Project Structure…", G.file, "Ctrl+Alt+Shift+S", "Cmd+;"],
  ["QuickChangeScheme", "Quick Switch Scheme…", G.view, "Ctrl+`", "Ctrl+`"],
  ["ChangeLaf", "Theme: Toggle Dark/Light", G.view, "", ""],
  ["ToggleFullScreen", "Toggle Full Screen Mode", G.view, "", "Ctrl+Cmd+F"],
  ["TogglePresentationMode", "Toggle Presentation Mode", G.view, "", ""],
  ["ToggleDistractionFreeMode", "Toggle Distraction Free Mode", G.view, "", ""],
  ["ToggleZenMode", "Toggle Zen Mode", G.view, "", ""],
  ["ShowNavBar", "Jump to Navigation Bar", G.view, "Alt+Home", "Cmd+Up"],
  ["ShowFilePath", "File Path", "Main Menu/File/File Properties", "Ctrl+Alt+F12", "Cmd+Alt+F12"],
  ["ShowPopupMenu", "Show Context Menu", G.other, "ContextMenu", ""],

  // ---- File
  ["NewElement", "New…", G.file, "Alt+Insert", "Cmd+N"],
  ["NewScratchFile", "New Scratch File", G.file, "Ctrl+Alt+Shift+Insert", "Cmd+Shift+N"],
  ["NewProject", "New Project…", G.file, "", ""],
  ["OpenFile", "Open…", G.file, "", ""],
  ["SaveAll", "Save All", G.file, "Ctrl+S", "Cmd+S"],
  ["Synchronize", "Reload All from Disk", G.file, "Ctrl+Alt+Y", "Cmd+Alt+Y"],
  ["CloseProject", "Close Project", G.file, "", ""],
  ["Exit", "Exit", G.file, "", "Cmd+Q"],
  ["InvalidateCaches", "Invalidate Caches…", G.file, "", ""],
  ["ExportSettings", "Export Settings…", "Main Menu/File/Manage IDE Settings", "", ""],

  // ---- Edit
  ["$Undo", "Undo", G.edit, "Ctrl+Z | Alt+Backspace", "Cmd+Z", N],
  ["$Redo", "Redo", G.edit, "Ctrl+Shift+Z | Alt+Shift+Backspace", "Cmd+Shift+Z", N],
  ["$Cut", "Cut", G.edit, "Ctrl+X | Shift+Delete", "Cmd+X", N],
  ["$Copy", "Copy", G.edit, "Ctrl+C | Ctrl+Insert", "Cmd+C", N],
  ["$Paste", "Paste", G.edit, "Ctrl+V | Shift+Insert", "Cmd+V", N],
  ["PasteMultiple", "Paste from History…", G.edit, "Ctrl+Shift+V", "Cmd+Shift+V"],
  ["PasteSimple", "Paste as Plain Text", G.edit, "Ctrl+Alt+Shift+V", "Cmd+Alt+Shift+V"],
  ["$SelectAll", "Select All", G.edit, "Ctrl+A", "Cmd+A", N],
  ["$Delete", "Delete", G.edit, "Delete", "Delete | Backspace", N],
  ["CopyPaths", "Copy Absolute Path", "Main Menu/Edit/Copy Path/Reference", "Ctrl+Shift+C", "Cmd+Shift+C"],
  ["CopyReference", "Copy Reference", "Main Menu/Edit/Copy Path/Reference", "Ctrl+Alt+Shift+C", "Cmd+Alt+Shift+C"],
  ["EditorToggleColumnMode", "Column Selection Mode", G.edit, "Alt+Shift+Insert", "Cmd+Shift+8"],
  ["SelectNextOccurrence", "Add Selection for Next Occurrence", G.edit, "Alt+J", "Ctrl+G", E],
  ["UnselectPreviousOccurrence", "Unselect Occurrence", G.edit, "Alt+Shift+J", "Ctrl+Shift+G", E],
  ["SelectAllOccurrences", "Select All Occurrences", G.edit, "Ctrl+Alt+Shift+J", "Ctrl+Cmd+G", E],
  ["EditorSelectWord", "Extend Selection", G.edit, "Ctrl+W", "Alt+Up", E],
  ["EditorUnSelectWord", "Shrink Selection", G.edit, "Ctrl+Shift+W", "Alt+Down", E],
  ["EditorJoinLines", "Join Lines", G.edit, "Ctrl+Shift+J", "Ctrl+Shift+J", E],
  ["EditorDuplicate", "Duplicate Line or Selection", G.edit, "Ctrl+D", "Cmd+D", E],
  ["EditorDeleteLine", "Delete Line", G.edit, "Ctrl+Y", "Cmd+Backspace", E],
  ["EditorToggleCase", "Toggle Case", G.edit, "Ctrl+Shift+U", "Cmd+Shift+U", E],
  ["EditorSortLines", "Sort Lines", G.edit, "", "", E],
  ["EditorReverseLines", "Reverse Lines", G.edit, "", "", E],
  ["FillParagraph", "Fill Paragraph", G.edit, "", "", E],

  // ---- Find
  ["Find", "Find…", G.find, "Ctrl+F", "Cmd+F"],
  ["Replace", "Replace…", G.find, "Ctrl+R", "Cmd+R", E],
  ["FindNext", "Find Next / Move to Next Occurrence", G.find, "F3", "Cmd+G"],
  ["FindPrevious", "Find Previous / Move to Previous Occurrence", G.find, "Shift+F3", "Cmd+Shift+G"],
  ["FindWordAtCaret", "Find Word at Caret", G.find, "Ctrl+F3", "Alt+Cmd+F3", E],
  ["FindInPath", "Find in Files…", G.find, "Ctrl+Shift+F", "Cmd+Shift+F"],
  ["ReplaceInPath", "Replace in Files…", G.find, "Ctrl+Shift+R", "Cmd+Shift+R"],
  ["FindUsages", "Find Usages", G.find, "Alt+F7", "Alt+F7"],
  ["FindUsagesInFile", "Find Usages in File", G.find, "Ctrl+F7", "Cmd+F7", E],
  ["HighlightUsagesInFile", "Highlight Usages in File", G.find, "Ctrl+Shift+F7", "Cmd+Shift+F7", E],
  ["ShowUsages", "Show Usages", G.find, "Ctrl+Alt+F7", "Cmd+Alt+F7"],
  ["ShowSettingsAndFindUsages", "Find Usages Settings…", G.find, "Ctrl+Alt+Shift+F7", "Cmd+Alt+Shift+F7"],
  ["StructuralSearchPlugin.StructuralSearchAction", "Search Structurally…", G.find, "", ""],

  // ---- View / tool windows
  ["ActivateProjectToolWindow", "Project", G.toolWindows, "Alt+1", "Cmd+1"],
  ["ActivateBookmarksToolWindow", "Bookmarks", G.toolWindows, "Alt+2", "Cmd+2"],
  ["ActivateFindToolWindow", "Find", G.toolWindows, "Alt+3", "Cmd+3"],
  ["ActivateRunToolWindow", "Run", G.toolWindows, "Alt+4", "Cmd+4"],
  ["ActivateDebugToolWindow", "Debug", G.toolWindows, "Alt+5", "Cmd+5"],
  ["ActivateProblemsViewToolWindow", "Problems", G.toolWindows, "Alt+6", "Cmd+6"],
  ["ActivateStructureToolWindow", "Structure", G.toolWindows, "Alt+7", "Cmd+7"],
  ["ActivateServicesToolWindow", "Services", G.toolWindows, "Alt+8", "Cmd+8"],
  ["ActivateVersionControlToolWindow", "Git", G.toolWindows, "Alt+9", "Cmd+9"],
  ["ActivateCommitToolWindow", "Commit", G.toolWindows, "Alt+0", "Cmd+0"],
  ["ActivateTerminalToolWindow", "Terminal", G.toolWindows, "Alt+F12", "Alt+F12"],
  ["ActivateDatabaseToolWindow", "Database", G.toolWindows, "", ""],
  ["ActivateNotificationsToolWindow", "Notifications", G.toolWindows, "", ""],
  ["ActivateAIChatToolWindow", "AI Chat", G.toolWindows, "", ""],
  ["ActivatePullRequestsToolWindow", "Pull Requests", G.toolWindows, "", ""],
  ["ActivateTODOToolWindow", "TODO", G.toolWindows, "", ""],
  ["QuickJavaDoc", "Quick Documentation", G.view, "Ctrl+Q", "F1 | Ctrl+J"],
  ["ExternalJavaDoc", "External Documentation", G.view, "Shift+F1", "Shift+F1"],
  ["QuickImplementations", "Quick Definition", G.view, "Ctrl+Shift+I", "Alt+Space | Cmd+Y"],
  ["ParameterInfo", "Parameter Info", G.view, "Ctrl+P", "Cmd+P", E],
  ["ExpressionTypeInfo", "Type Info", G.view, "Ctrl+Shift+P", "Ctrl+Shift+P", E],
  ["EditorContextInfo", "Context Info", G.view, "Alt+Q", "Ctrl+Shift+Q", E],
  ["ShowErrorDescription", "Error Description", G.view, "Ctrl+F1", "Cmd+F1", E],
  ["RecentFiles", "Recent Files", G.view, "Ctrl+E", "Cmd+E"],
  ["RecentChangedFiles", "Recently Changed Files", G.view, "", ""],
  ["RecentLocations", "Recent Locations", G.view, "Ctrl+Shift+E", "Cmd+Shift+E"],
  ["Switcher", "Switcher", G.view, "Ctrl+Tab | Ctrl+Shift+Tab", "Ctrl+Tab | Ctrl+Shift+Tab"],
  ["ToggleRenderedDocPresentation", "Toggle Rendered View", G.view, "Ctrl+Alt+Q", "Ctrl+Alt+Q", E],
  ["CompareTwoFiles", "Compare Files", G.view, "Ctrl+D", "Cmd+D", TW],
  ["ViewInplaceComments", "Inlay Hints", G.view, "", ""],

  // ---- Navigate
  ["GotoClass", "Class…", G.navigate, "Ctrl+N", "Cmd+O"],
  ["GotoFile", "File…", G.navigate, "Ctrl+Shift+N", "Cmd+Shift+O"],
  ["GotoSymbol", "Symbol…", G.navigate, "Ctrl+Alt+Shift+N", "Cmd+Alt+O"],
  ["GotoLine", "Line/Column…", G.navigate, "Ctrl+G", "Cmd+L", E],
  ["Back", "Back", G.navigate, "Ctrl+Alt+Left", "Cmd+[ | Cmd+Alt+Left"],
  ["Forward", "Forward", G.navigate, "Ctrl+Alt+Right", "Cmd+] | Cmd+Alt+Right"],
  ["JumpToLastChange", "Last Edit Location", G.navigate, "Ctrl+Shift+Backspace", "Cmd+Shift+Backspace"],
  ["JumpToNextChange", "Next Edit Location", G.navigate, "", ""],
  ["SelectIn", "Select In…", G.navigate, "Alt+F1", "Alt+F1"],
  ["GotoDeclaration", "Declaration or Usages", G.navigate, "Ctrl+B", "Cmd+B", E],
  ["GotoImplementation", "Implementation(s)", G.navigate, "Ctrl+Alt+B", "Cmd+Alt+B", E],
  ["GotoTypeDeclaration", "Type Declaration", G.navigate, "Ctrl+Shift+B", "Cmd+Shift+B", E],
  ["GotoSuperMethod", "Super Method", G.navigate, "Ctrl+U", "Cmd+U", E],
  ["GotoTest", "Test", G.navigate, "Ctrl+Shift+T", "Cmd+Shift+T", E],
  ["GotoRelated", "Related Symbol…", G.navigate, "Ctrl+Alt+Home", "Ctrl+Cmd+Up", E],
  ["FileStructurePopup", "File Structure", G.navigate, "Ctrl+F12", "Cmd+F12"],
  ["TypeHierarchy", "Type Hierarchy", G.navigate, "Ctrl+H", "Ctrl+H"],
  ["MethodHierarchy", "Method Hierarchy", G.navigate, "Ctrl+Shift+H", "Cmd+Shift+H"],
  ["CallHierarchy", "Call Hierarchy", G.navigate, "Ctrl+Alt+H", "Ctrl+Alt+H"],
  ["GotoNextError", "Next Highlighted Error", G.navigate, "F2", "F2", E],
  ["GotoPreviousError", "Previous Highlighted Error", G.navigate, "Shift+F2", "Shift+F2", E],
  ["MethodDown", "Next Method", G.navigate, "Alt+Down", "Ctrl+Down", E],
  ["MethodUp", "Previous Method", G.navigate, "Alt+Up", "Ctrl+Up", E],
  ["NextOccurence", "Next Occurrence", G.navigate, "Ctrl+Alt+Down", "Cmd+Alt+Down"],
  ["PreviousOccurence", "Previous Occurrence", G.navigate, "Ctrl+Alt+Up", "Cmd+Alt+Up"],
  ["EditSource", "Jump to Source", G.navigate, "F4", "F4 | Cmd+Down"],
  ["EditorMatchBrace", "Move Caret to Matching Brace", G.navigate, "Ctrl+Shift+M", "Ctrl+M", E],
  ["EditorCodeBlockStart", "Move Caret to Code Block Start", G.navigate, "Ctrl+[", "Cmd+Alt+[", E],
  ["EditorCodeBlockEnd", "Move Caret to Code Block End", G.navigate, "Ctrl+]", "Cmd+Alt+]", E],
  ["EditorCodeBlockStartWithSelection", "Move Caret to Code Block Start with Selection", G.navigate, "Ctrl+Shift+[", "Cmd+Alt+Shift+[", E],
  ["EditorCodeBlockEndWithSelection", "Move Caret to Code Block End with Selection", G.navigate, "Ctrl+Shift+]", "Cmd+Alt+Shift+]", E],
  ["VcsShowNextChangeMarker", "Next Change", G.navigate, "Ctrl+Alt+Shift+Down", "Ctrl+Alt+Shift+Down", E],
  ["VcsShowPrevChangeMarker", "Previous Change", G.navigate, "Ctrl+Alt+Shift+Up", "Ctrl+Alt+Shift+Up", E],

  // ---- Bookmarks
  ["ToggleBookmark", "Toggle Bookmark", G.bookmarks, "F11", "F3"],
  ["ToggleBookmarkWithMnemonic", "Toggle Bookmark with Mnemonic", G.bookmarks, "Ctrl+F11", "Alt+F3"],
  ["ShowBookmarks", "Show All Bookmarks", G.bookmarks, "Shift+F11", "Cmd+F3"],
  ...Array.from({ length: 10 }, (_, i): Row => [`GotoBookmark${i}`, `Go to Bookmark ${i}`, G.bookmarks, `Ctrl+${i}`, `Ctrl+${i}`]),
  ...Array.from({ length: 10 }, (_, i): Row => [`ToggleBookmark${i}`, `Toggle Bookmark ${i}`, G.bookmarks, `Ctrl+Shift+${i}`, `Ctrl+Shift+${i}`]),

  // ---- Code
  ["CodeCompletion", "Basic Completion", G.code, "Ctrl+Space", "Ctrl+Space", E],
  ["SmartTypeCompletion", "Type-Matching Completion", G.code, "Ctrl+Shift+Space", "Ctrl+Shift+Space", E],
  ["HippieCompletion", "Cyclic Expand Word", G.code, "Alt+/", "Alt+/", E],
  ["HippieBackwardCompletion", "Cyclic Expand Word (Backward)", G.code, "Alt+Shift+/", "Alt+Shift+/", E],
  ["EditorCompleteStatement", "Complete Current Statement", G.code, "Ctrl+Shift+Enter", "Cmd+Shift+Enter", E],
  ["Generate", "Generate…", G.code, "Alt+Insert", "Cmd+N", E],
  ["OverrideMethods", "Override Methods…", G.code, "Ctrl+O", "Ctrl+O", E],
  ["ImplementMethods", "Implement Methods…", G.code, "Ctrl+I", "Ctrl+I", E],
  ["SurroundWith", "Surround With…", G.code, "Ctrl+Alt+T", "Cmd+Alt+T", E],
  ["Unwrap", "Unwrap/Remove…", G.code, "Ctrl+Shift+Delete", "Cmd+Shift+Delete", E],
  ["InsertLiveTemplate", "Insert Live Template…", G.code, "Ctrl+J", "Cmd+J", E],
  ["SurroundWithLiveTemplate", "Surround with Live Template…", G.code, "Ctrl+Alt+J", "Cmd+Alt+J", E],
  ["CommentByLineComment", "Comment with Line Comment", G.code, "Ctrl+/ | Ctrl+NumPad/", "Cmd+/ | Cmd+NumPad/", E],
  ["CommentByBlockComment", "Comment with Block Comment", G.code, "Ctrl+Shift+/ | Ctrl+Shift+NumPad/", "Cmd+Alt+/ | Cmd+Alt+NumPad/", E],
  ["ReformatCode", "Reformat Code", G.code, "Ctrl+Alt+L", "Cmd+Alt+L"],
  ["ShowReformatFileDialog", "Reformat File…", G.code, "Ctrl+Alt+Shift+L", "Cmd+Alt+Shift+L"],
  ["AutoIndentLines", "Auto-Indent Lines", G.code, "Ctrl+Alt+I", "Ctrl+Alt+I", E],
  ["OptimizeImports", "Optimize Imports", G.code, "Ctrl+Alt+O", "Ctrl+Alt+O"],
  ["RearrangeCode", "Rearrange Code", G.code, "", ""],
  ["MoveStatementUp", "Move Statement Up", G.code, "Ctrl+Shift+Up", "Cmd+Shift+Up", E],
  ["MoveStatementDown", "Move Statement Down", G.code, "Ctrl+Shift+Down", "Cmd+Shift+Down", E],
  ["MoveElementLeft", "Move Element Left", G.code, "Ctrl+Alt+Shift+Left", "Cmd+Alt+Shift+Left", E],
  ["MoveElementRight", "Move Element Right", G.code, "Ctrl+Alt+Shift+Right", "Cmd+Alt+Shift+Right", E],
  ["MoveLineUp", "Move Line Up", G.code, "Alt+Shift+Up", "Alt+Shift+Up", E],
  ["MoveLineDown", "Move Line Down", G.code, "Alt+Shift+Down", "Alt+Shift+Down", E],
  ["InspectCode", "Inspect Code…", G.code, "", ""],
  ["CodeInspection.OnEditor", "Code Cleanup / Inspect File", G.code, "Alt+Shift+I", "Ctrl+Alt+Shift+I", E],
  ["RunInspection", "Run Inspection by Name…", G.code, "Ctrl+Alt+Shift+I", "Cmd+Alt+Shift+I"],
  ["ShowIntentionsGroup", "Show Quick-Fixes", G.code, "", "", E],

  // ---- Folding
  ["ExpandRegion", "Expand", G.folding, "Ctrl+= | Ctrl+NumPad+", "Cmd+= | Cmd+NumPad+", E],
  ["CollapseRegion", "Collapse", G.folding, "Ctrl+- | Ctrl+NumPad-", "Cmd+- | Cmd+NumPad-", E],
  ["ExpandRegionRecursively", "Expand Recursively", G.folding, "Ctrl+Alt+= | Ctrl+Alt+NumPad+", "Cmd+Alt+= | Cmd+Alt+NumPad+", E],
  ["CollapseRegionRecursively", "Collapse Recursively", G.folding, "Ctrl+Alt+- | Ctrl+Alt+NumPad-", "Cmd+Alt+- | Cmd+Alt+NumPad-", E],
  ["ExpandAllRegions", "Expand All", G.folding, "Ctrl+Shift+= | Ctrl+Shift+NumPad+", "Cmd+Shift+= | Cmd+Shift+NumPad+", E],
  ["CollapseAllRegions", "Collapse All", G.folding, "Ctrl+Shift+- | Ctrl+Shift+NumPad-", "Cmd+Shift+- | Cmd+Shift+NumPad-", E],
  ["CollapseSelection", "Fold Selection / Remove Region", G.folding, "Ctrl+.", "Cmd+.", E],
  ["CollapseDocComments", "Collapse Doc Comments", G.folding, "", "", E],

  // ---- Refactor
  ["Refactorings.QuickListPopupAction", "Refactor This…", G.refactor, "Ctrl+Alt+Shift+T", "Ctrl+T"],
  ["RenameElement", "Rename…", G.refactor, "Shift+F6", "Shift+F6"],
  ["RenameFile", "Rename File…", G.refactor, "", ""],
  ["ChangeSignature", "Change Signature…", G.refactor, "Ctrl+F6", "Cmd+F6"],
  ["Move", "Move…", G.refactor, "F6", "F6"],
  ["CopyElement", "Copy File…", G.refactor, "F5", "F5"],
  ["SafeDelete", "Safe Delete…", G.refactor, "Alt+Delete", "Cmd+Delete"],
  ["Inline", "Inline…", G.refactor, "Ctrl+Alt+N", "Cmd+Alt+N", E],
  ["ExtractMethod", "Extract Method…", "Main Menu/Refactor/Extract/Introduce", "Ctrl+Alt+M", "Cmd+Alt+M", E],
  ["IntroduceVariable", "Introduce Variable…", "Main Menu/Refactor/Extract/Introduce", "Ctrl+Alt+V", "Cmd+Alt+V", E],
  ["IntroduceField", "Introduce Field…", "Main Menu/Refactor/Extract/Introduce", "Ctrl+Alt+F", "Cmd+Alt+F", E],
  ["IntroduceConstant", "Introduce Constant…", "Main Menu/Refactor/Extract/Introduce", "Ctrl+Alt+C", "Cmd+Alt+C", E],
  ["IntroduceParameter", "Introduce Parameter…", "Main Menu/Refactor/Extract/Introduce", "Ctrl+Alt+P", "Cmd+Alt+P", E],
  ["ExtractInterface", "Extract Interface…", "Main Menu/Refactor/Extract/Introduce", "", ""],
  ["ExtractSuperclass", "Extract Superclass…", "Main Menu/Refactor/Extract/Introduce", "", ""],
  ["MemberPushDown", "Push Members Down…", G.refactor, "", ""],
  ["MembersPullUp", "Pull Members Up…", G.refactor, "", ""],

  // ---- Run / debug
  ["Run", "Run", G.run, "Shift+F10", "Ctrl+R"],
  ["Debug", "Debug", G.run, "Shift+F9", "Ctrl+D"],
  ["ChooseRunConfiguration", "Run…", G.run, "Alt+Shift+F10", "Ctrl+Alt+R"],
  ["ChooseDebugConfiguration", "Debug…", G.run, "Alt+Shift+F9", "Ctrl+Alt+D"],
  ["RunClass", "Run Context Configuration", G.run, "Ctrl+Shift+F10", "Ctrl+Shift+R"],
  ["DebugClass", "Debug Context Configuration", G.run, "Ctrl+Shift+F9", "Ctrl+Shift+D"],
  ["editRunConfigurations", "Edit Configurations…", G.run, "", ""],
  ["Stop", "Stop", G.run, "Ctrl+F2", "Cmd+F2"],
  ["Rerun", "Rerun", G.run, "Ctrl+F5", "Cmd+R", TW],
  ["CompileDirty", "Build Project", "Main Menu/Build", "Ctrl+F9", "Cmd+F9"],
  ["ToggleLineBreakpoint", "Toggle Line Breakpoint", "Main Menu/Run/Toggle Breakpoint", "Ctrl+F8", "Cmd+F8", E],
  ["ViewBreakpoints", "View Breakpoints…", G.run, "Ctrl+Shift+F8", "Cmd+Shift+F8"],
  ["XDebugger.MuteBreakpoints", "Mute Breakpoints", G.debugger, "", ""],
  ["StepOver", "Step Over", G.debugger, "F8", "F8"],
  ["StepInto", "Step Into", G.debugger, "F7", "F7"],
  ["SmartStepInto", "Smart Step Into", G.debugger, "Shift+F7", "Shift+F7"],
  ["ForceStepInto", "Force Step Into", G.debugger, "Alt+Shift+F7", "Alt+Shift+F7"],
  ["StepOut", "Step Out", G.debugger, "Shift+F8", "Shift+F8"],
  ["RunToCursor", "Run to Cursor", G.debugger, "Alt+F9", "Alt+F9"],
  ["ForceRunToCursor", "Force Run to Cursor", G.debugger, "Ctrl+Alt+F9", "Cmd+Alt+F9"],
  ["Resume", "Resume Program", G.debugger, "F9", "Cmd+Alt+R"],
  ["Pause", "Pause Program", G.debugger, "", ""],
  ["EvaluateExpression", "Evaluate Expression…", G.debugger, "Alt+F8", "Alt+F8"],
  ["QuickEvaluateExpression", "Quick Evaluate Expression", G.debugger, "Ctrl+Alt+F8", "Cmd+Alt+F8", E],
  ["ShowExecutionPoint", "Show Execution Point", G.debugger, "Alt+F10", "Alt+F10"],
  ["Debugger.AddToWatch", "Add to Watches", G.debugger, "", ""],
  ["XDebugger.AttachToProcess", "Attach to Process…", G.run, "Ctrl+Alt+F5", "Cmd+Alt+F5"],

  // ---- Git
  ["CheckinProject", "Commit…", G.git, "Ctrl+K", "Cmd+K"],
  ["Vcs.Push", "Push…", G.git, "Ctrl+Shift+K", "Cmd+Shift+K"],
  ["Vcs.UpdateProject", "Update Project…", G.git, "Ctrl+T", "Cmd+T"],
  ["Git.Commit.And.Push.Executor", "Commit and Push…", G.git, "Ctrl+Alt+K", "Cmd+Alt+K"],
  ["Vcs.QuickListPopupAction", "VCS Operations Popup…", G.git, "Alt+`", "Ctrl+V"],
  ["Git.Branches", "Branches…", G.git, "", ""],
  ["Git.Fetch", "Fetch", G.git, "", ""],
  ["Git.Pull", "Pull…", G.git, "", ""],
  ["Git.Stash", "Stash Changes…", G.git, "", ""],
  ["Git.Unstash", "Unstash Changes…", G.git, "", ""],
  ["Git.CreateNewBranch", "New Branch…", G.git, "", ""],
  ["ChangesView.Rollback", "Rollback…", G.git, "Ctrl+Alt+Z", "Cmd+Alt+Z"],
  ["Annotate", "Annotate with Git Blame", "Main Menu/Git/Current File", "", ""],
  ["Vcs.ShowTabbedFileHistory", "Show History", "Main Menu/Git/Current File", "", ""],
  ["Compare.SameVersion", "Compare with Revision…", "Main Menu/Git/Current File", "", ""],
  ["Vcs.ShowHistoryForBlock", "Show History for Selection", "Main Menu/Git/Current File", "", "", E],
  ["Vcs.RollbackChangedLines", "Rollback Lines", "Main Menu/Git/Current File", "Ctrl+Alt+Z", "Cmd+Alt+Z", E],
  ["Vcs.ShowMessageHistory", "Show Commit Message History", G.git, "Ctrl+M", "Cmd+M", TW],
  ["LocalHistory.ShowHistory", "Show Local History", "Main Menu/File/Local History", "", ""],

  // ---- Window
  ["HideAllWindows", "Hide All Tool Windows", G.window, "Ctrl+Shift+F12", "Cmd+Shift+F12"],
  ["JumpToLastWindow", "Jump to Last Tool Window", G.window, "F12", "F12"],
  ["HideActiveWindow", "Hide Active Tool Window", G.window, "Shift+Escape", "Shift+Escape"],
  ["HideSideWindows", "Hide Side Tool Windows", G.window, "", ""],
  ["MaximizeToolWindow", "Maximize Tool Window", G.window, "Ctrl+Shift+'", "Cmd+Shift+'"],
  ["ResizeToolWindowLeft", "Stretch to Left", "Main Menu/Window/Active Tool Window", "Ctrl+Shift+Left", "Ctrl+Alt+Shift+Left", TW],
  ["ResizeToolWindowRight", "Stretch to Right", "Main Menu/Window/Active Tool Window", "Ctrl+Shift+Right", "Ctrl+Alt+Shift+Right", TW],
  ["ResizeToolWindowUp", "Stretch to Top", "Main Menu/Window/Active Tool Window", "Ctrl+Shift+Up", "Ctrl+Alt+Shift+Up", TW],
  ["ResizeToolWindowDown", "Stretch to Bottom", "Main Menu/Window/Active Tool Window", "Ctrl+Shift+Down", "Ctrl+Alt+Shift+Down", TW],
  ["RestoreDefaultLayout", "Restore Default Layout", G.window, "Shift+F12", "Shift+F12"],
  ["StoreDefaultLayout", "Store Current Layout as Default", G.window, "", ""],
  ["NextTab", "Select Next Tab", G.editorTabs, "Alt+Right", "Ctrl+Right | Cmd+Shift+]"],
  ["PreviousTab", "Select Previous Tab", G.editorTabs, "Alt+Left", "Ctrl+Left | Cmd+Shift+["],
  ["CloseContent", "Close Tab", G.editorTabs, "Ctrl+F4", "Cmd+W"],
  ["CloseAllEditors", "Close All Tabs", G.editorTabs, "", ""],
  ["CloseAllEditorsButActive", "Close Other Tabs", G.editorTabs, "", "Cmd+Alt+W"],
  ["CloseAllUnmodifiedEditors", "Close Unmodified Tabs", G.editorTabs, "", ""],
  ["ReopenClosedTab", "Reopen Closed Tab", G.editorTabs, "", ""],
  ["PinActiveEditorTab", "Pin Tab", G.editorTabs, "", ""],
  ["CloseActiveTab", "Close Active Tool Window Tab", G.window, "Ctrl+Shift+F4", "Cmd+Shift+F4"],
  ["SplitVertically", "Split Right", G.editorTabs, "", ""],
  ["SplitHorizontally", "Split Down", G.editorTabs, "", ""],
  ["NextSplitter", "Goto Next Splitter", G.editorTabs, "", "Alt+Tab"],
  ["PrevSplitter", "Goto Previous Splitter", G.editorTabs, "", "Alt+Shift+Tab"],
  ["NextProjectWindow", "Next Project Window", G.window, "Ctrl+Alt+]", "Cmd+`"],
  ["PreviousProjectWindow", "Previous Project Window", G.window, "Ctrl+Alt+[", "Cmd+Shift+`"],

  // ---- Editor actions (typing & caret, mostly native)
  ["EditorSplitLine", "Split Line", G.editor, "Ctrl+Enter", "Cmd+Enter", E],
  ["EditorStartNewLine", "Start New Line", G.editor, "Shift+Enter", "Shift+Enter", E],
  ["EditorStartNewLineBefore", "Start New Line Before Current", G.editor, "Ctrl+Alt+Enter", "Cmd+Alt+Enter", E],
  ["EditorScrollUp", "Scroll Up", G.editor, "Ctrl+Up", "", E],
  ["EditorScrollDown", "Scroll Down", G.editor, "Ctrl+Down", "", E],
  ["EditorDeleteToWordStart", "Delete to Word Start", G.editor, "Ctrl+Backspace", "Alt+Backspace", EN],
  ["EditorDeleteToWordEnd", "Delete to Word End", G.editor, "Ctrl+Delete", "Alt+Delete", EN],
  ["EditorDeleteToLineStart", "Delete to Line Start", G.editor, "", "", EN],
  ["EditorDeleteToLineEnd", "Delete to Line End", G.editor, "", "Ctrl+K", EN],
  ["EditorIndentSelection", "Indent Selection", G.editor, "Tab", "Tab", EN],
  ["EditorUnindentSelection", "Unindent Line or Selection", G.editor, "Shift+Tab", "Shift+Tab", EN],
  ["EditorEnter", "Enter", G.editor, "Enter", "Enter", EN],
  ["EditorEscape", "Escape", G.editor, "Escape", "Escape", EN],
  ["EditorBackSpace", "Backspace", G.editor, "Backspace | Shift+Backspace", "Backspace | Shift+Backspace", EN],
  ["EditorLineStart", "Move Caret to Line Start", G.editor, "Home", "Home | Cmd+Left", EN],
  ["EditorLineEnd", "Move Caret to Line End", G.editor, "End", "End | Cmd+Right", EN],
  ["EditorTextStart", "Move Caret to Text Start", G.editor, "Ctrl+Home", "Cmd+Home", EN],
  ["EditorTextEnd", "Move Caret to Text End", G.editor, "Ctrl+End", "Cmd+End", EN],
  ["EditorPreviousWord", "Move Caret to Previous Word", G.editor, "Ctrl+Left", "Alt+Left", EN],
  ["EditorNextWord", "Move Caret to Next Word", G.editor, "Ctrl+Right", "Alt+Right", EN],
  ["EditorPreviousWordWithSelection", "Move Caret to Previous Word with Selection", G.editor, "Ctrl+Shift+Left", "Alt+Shift+Left", EN],
  ["EditorNextWordWithSelection", "Move Caret to Next Word with Selection", G.editor, "Ctrl+Shift+Right", "Alt+Shift+Right", EN],
  ["EditorPageUp", "Page Up", G.editor, "PageUp", "PageUp | Fn+Up", EN],
  ["EditorPageDown", "Page Down", G.editor, "PageDown", "PageDown", EN],
  ["EditorMoveToPageTop", "Move Caret to Page Top", G.editor, "Ctrl+PageUp", "Cmd+PageUp", EN],
  ["EditorMoveToPageBottom", "Move Caret to Page Bottom", G.editor, "Ctrl+PageDown", "Cmd+PageDown", EN],
  ["EditorCloneCaretAbove", "Clone Caret Above", G.editor, "", "", E],
  ["EditorCloneCaretBelow", "Clone Caret Below", G.editor, "", "", E],
  ["EditorToggleInsertState", "Toggle Insert/Overwrite", G.editor, "Insert", "", EN],
  ["EditorChooseLookupItemReplace", "Choose Lookup Item and Replace", G.editor, "Tab", "Tab", EN],
];

function splitShortcuts(s: string): string[] {
  return s
    .split("|")
    .map((x) => x.trim())
    // "Fn" is not a key the UI can see; such alternatives are dropped.
    .filter((x) => x && !/\bFn\+/.test(x));
}

export const PHPSTORM_ACTIONS: ActionDef[] = ROWS.map(([id, title, group, , , opts]) => ({
  id,
  title,
  group,
  context: opts?.context ?? "any",
  native: opts?.native ?? false,
  description: opts?.description,
}));

export const DEFAULT_KEYMAPS: Record<KeymapId, Record<string, string[]>> = {
  $default: Object.fromEntries(ROWS.map(([id, , , win]) => [id, splitShortcuts(win)])),
  macOS: Object.fromEntries(ROWS.map(([id, , , , mac]) => [id, splitShortcuts(mac)])),
};

/**
 * Ids that Forge features already register under another name, plus common alternate
 * spellings. `runAction` follows these when the PHPStorm id itself has no command.
 */
export const ACTION_ALIASES: Record<string, string[]> = {
  SearchEverywhere: ["search.everywhere"],
  ShowSettings: ["settings.open"],
  Run: ["run.run"],
  Debug: ["run.debug"],
  editRunConfigurations: ["run.editConfigurations"],
  "Git.Branches": ["git.branches"],
  IntroduceVariable: ["ExtractVariable"],
};
