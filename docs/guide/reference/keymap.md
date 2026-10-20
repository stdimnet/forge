---
title: Keymap reference
description: Every action of Forge's PHPStorm keymap with its default shortcuts on Windows/Linux and macOS, grouped like PHPStorm's keymap, and whether Forge implements it.
order: 10
tags: [keymap, shortcuts, reference]
---

# Keymap reference

Forge's default keymaps are PHPStorm's (`crates/forge-keymap/data/phpstorm.json`), keyed by
PHPStorm's action ids. This page is the complete list. To learn the shortcuts the way the
JetBrains help teaches them, start with:

- [Keymap reference card](../user/shortcuts/reference-card.md): the 60 most important shortcuts
  on one printable page;
- [Shortcuts by topic](../user/shortcuts/index.md): one page per area (editing, navigation,
  search, refactoring, run and debug, Git, tool windows, general) that explains every
  implemented action, where it is in the menus, and shows its result;
- [Main menu reference](main-menu/index.md): every menu entry with its shortcut and status;
- [Keymap](../user/keymap.md): changing shortcuts, chords and focus contexts.

## How to read the tables

Actions are grouped as in Settings | Keymap. The **In Forge** column says whether the action does
something on this version (checked against the commands the UI registers):

- **yes**: implemented;
- **native**: handled by the editor or the operating system (undo, clipboard, caret movement);
- **—**: not implemented yet; the shortcut stays reserved and shows "*Action* is not available
  yet", and its main menu entry is disabled.

Shortcuts marked `Fn+…` in PHPStorm are left out. Several Forge commands implement PHPStorm
actions under another id (aliases): `search.everywhere` → `SearchEverywhere`, `settings.open` →
`ShowSettings`, `run.run` → `Run`, `run.debug` → `Debug`, `run.editConfigurations` →
`editRunConfigurations`, `git.branches` → `Git.Branches`, `git.commit` → `CheckinProject`,
`git.push` → `Vcs.Push`, `git.update` → `Vcs.UpdateProject`, `git.fetch` → `Git.Fetch`,
`git.newBranch` → `Git.CreateNewBranch`, `git.log` → `Vcs.Show.Log`, `git.annotate` →
`Annotate`, `php.generate` → `Generate`, `workspace.open` → `OpenFile`, `workspace.close` →
`CloseProject`, `ExtractVariable` → `IntroduceVariable`. `Exit` exists only in the desktop app.

## Main Menu


### Navigate

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| Search Everywhere | `SearchEverywhere` | `Double Shift` | `Double Shift` | yes |
| Class… | `GotoClass` | `Ctrl+N` | `Cmd+O` | yes |
| File… | `GotoFile` | `Ctrl+Shift+N` | `Cmd+Shift+O` | yes |
| Symbol… | `GotoSymbol` | `Ctrl+Alt+Shift+N` | `Cmd+Alt+O` | yes |
| Line/Column… | `GotoLine` | `Ctrl+G` | `Cmd+L` | yes |
| Back | `Back` | `Ctrl+Alt+Left` | `Cmd+[`, `Cmd+Alt+Left` | yes |
| Forward | `Forward` | `Ctrl+Alt+Right` | `Cmd+]`, `Cmd+Alt+Right` | yes |
| Last Edit Location | `JumpToLastChange` | `Ctrl+Shift+Backspace` | `Cmd+Shift+Backspace` | — |
| Next Edit Location | `JumpToNextChange` |  |  | — |
| Select In… | `SelectIn` | `Alt+F1` | `Alt+F1` | — |
| Declaration or Usages | `GotoDeclaration` | `Ctrl+B` | `Cmd+B` | yes |
| Implementation(s) | `GotoImplementation` | `Ctrl+Alt+B` | `Cmd+Alt+B` | — |
| Type Declaration | `GotoTypeDeclaration` | `Ctrl+Shift+B` | `Cmd+Shift+B` | — |
| Super Method | `GotoSuperMethod` | `Ctrl+U` | `Cmd+U` | — |
| Test | `GotoTest` | `Ctrl+Shift+T` | `Cmd+Shift+T` | — |
| Related Symbol… | `GotoRelated` | `Ctrl+Alt+Home` | `Ctrl+Cmd+Up` | — |
| File Structure | `FileStructurePopup` | `Ctrl+F12` | `Cmd+F12` | — |
| Type Hierarchy | `TypeHierarchy` | `Ctrl+H` | `Ctrl+H` | — |
| Method Hierarchy | `MethodHierarchy` | `Ctrl+Shift+H` | `Cmd+Shift+H` | — |
| Call Hierarchy | `CallHierarchy` | `Ctrl+Alt+H` | `Ctrl+Alt+H` | — |
| Next Highlighted Error | `GotoNextError` | `F2` | `F2` | yes |
| Previous Highlighted Error | `GotoPreviousError` | `Shift+F2` | `Shift+F2` | yes |
| Next Method | `MethodDown` | `Alt+Down` | `Ctrl+Down` | — |
| Previous Method | `MethodUp` | `Alt+Up` | `Ctrl+Up` | — |
| Next Occurrence | `NextOccurence` | `Ctrl+Alt+Down` | `Cmd+Alt+Down` | — |
| Previous Occurrence | `PreviousOccurence` | `Ctrl+Alt+Up` | `Cmd+Alt+Up` | — |
| Jump to Source | `EditSource` | `F4` | `F4`, `Cmd+Down` | — |
| Move Caret to Matching Brace | `EditorMatchBrace` | `Ctrl+Shift+M` | `Ctrl+M` | yes |
| Move Caret to Code Block Start | `EditorCodeBlockStart` | `Ctrl+[` | `Cmd+Alt+[` | — |
| Move Caret to Code Block End | `EditorCodeBlockEnd` | `Ctrl+]` | `Cmd+Alt+]` | — |
| Move Caret to Code Block Start with Selection | `EditorCodeBlockStartWithSelection` | `Ctrl+Shift+[` | `Cmd+Alt+Shift+[` | — |
| Move Caret to Code Block End with Selection | `EditorCodeBlockEndWithSelection` | `Ctrl+Shift+]` | `Cmd+Alt+Shift+]` | — |
| Next Change | `VcsShowNextChangeMarker` | `Ctrl+Alt+Shift+Down` | `Ctrl+Alt+Shift+Down` | — |
| Previous Change | `VcsShowPrevChangeMarker` | `Ctrl+Alt+Shift+Up` | `Ctrl+Alt+Shift+Up` | — |

### Help

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| Find Action… | `GotoAction` | `Ctrl+Shift+A` | `Cmd+Shift+A` | yes |
| Keymap Reference | `Help.KeymapReference` |  |  | yes |
| About | `About` |  |  | yes |

### Code

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| Show Context Actions | `ShowIntentionActions` | `Alt+Enter` | `Alt+Enter` | — |
| Basic Completion | `CodeCompletion` | `Ctrl+Space`, `Shift+Space` | `Ctrl+Space`, `Shift+Space` | yes |
| Type-Matching Completion | `SmartTypeCompletion` | `Ctrl+Shift+Space` | `Ctrl+Shift+Space` | — |
| Cyclic Expand Word | `HippieCompletion` | `Alt+/` | `Alt+/` | — |
| Cyclic Expand Word (Backward) | `HippieBackwardCompletion` | `Alt+Shift+/` | `Alt+Shift+/` | — |
| Complete Current Statement | `EditorCompleteStatement` | `Ctrl+Shift+Enter` | `Cmd+Shift+Enter` | — |
| Generate… | `Generate` | `Alt+Insert` | `Cmd+N` | yes |
| Override Methods… | `OverrideMethods` | `Ctrl+O` | `Ctrl+O` | PHP editors¹ |
| Implement Methods… | `ImplementMethods` | `Ctrl+I` | `Ctrl+I` | PHP editors¹ |
| Surround With… | `SurroundWith` | `Ctrl+Alt+T` | `Cmd+Alt+T` | — |
| Unwrap/Remove… | `Unwrap` | `Ctrl+Shift+Delete` | `Cmd+Shift+Delete` | — |
| Insert Live Template… | `InsertLiveTemplate` | `Ctrl+J` | `Cmd+J` | — |
| Surround with Live Template… | `SurroundWithLiveTemplate` | `Ctrl+Alt+J` | `Cmd+Alt+J` | — |
| Comment with Line Comment | `CommentByLineComment` | `Ctrl+/`, `Ctrl+NumPad/` | `Cmd+/`, `Cmd+NumPad/` | yes |
| Comment with Block Comment | `CommentByBlockComment` | `Ctrl+Shift+/`, `Ctrl+Shift+NumPad/` | `Cmd+Alt+/`, `Cmd+Alt+NumPad/` | yes |
| Reformat Code | `ReformatCode` | `Ctrl+Alt+L` | `Cmd+Alt+L` | yes |
| Reformat File… | `ShowReformatFileDialog` | `Ctrl+Alt+Shift+L` | `Cmd+Alt+Shift+L` | — |
| Auto-Indent Lines | `AutoIndentLines` | `Ctrl+Alt+I` | `Ctrl+Alt+I` | yes |
| Optimize Imports | `OptimizeImports` | `Ctrl+Alt+O` | `Ctrl+Alt+O` | — |
| Rearrange Code | `RearrangeCode` |  |  | — |
| Move Statement Up | `MoveStatementUp` | `Ctrl+Shift+Up` | `Cmd+Shift+Up` | yes |
| Move Statement Down | `MoveStatementDown` | `Ctrl+Shift+Down` | `Cmd+Shift+Down` | yes |
| Move Element Left | `MoveElementLeft` | `Ctrl+Alt+Shift+Left` | `Cmd+Alt+Shift+Left` | — |
| Move Element Right | `MoveElementRight` | `Ctrl+Alt+Shift+Right` | `Cmd+Alt+Shift+Right` | — |
| Move Line Up | `MoveLineUp` | `Alt+Shift+Up` | `Alt+Shift+Up` | yes |
| Move Line Down | `MoveLineDown` | `Alt+Shift+Down` | `Alt+Shift+Down` | yes |
| Inspect Code… | `InspectCode` |  |  | — |
| Code Cleanup / Inspect File | `CodeInspection.OnEditor` | `Alt+Shift+I` | `Ctrl+Alt+Shift+I` | — |
| Run Inspection by Name… | `RunInspection` | `Ctrl+Alt+Shift+I` | `Cmd+Alt+Shift+I` | — |
| Show Quick-Fixes | `ShowIntentionsGroup` |  |  | — |

### File

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| Settings… | `ShowSettings` | `Ctrl+Alt+S` | `Cmd+,` | yes |
| Project Structure… | `ShowProjectStructureSettings` | `Ctrl+Alt+Shift+S` | `Cmd+;` | — |
| New… | `NewElement` | `Alt+Insert` | `Cmd+N` | — |
| New Scratch File | `NewScratchFile` | `Ctrl+Alt+Shift+Insert` | `Cmd+Shift+N` | yes |
| New Project… | `NewProject` |  |  | yes |
| Open… | `OpenFile` |  |  | yes |
| Save All | `SaveAll` | `Ctrl+S` | `Cmd+S` | yes |
| Reload All from Disk | `Synchronize` | `Ctrl+Alt+Y` | `Cmd+Alt+Y` | — |
| Close Project | `CloseProject` |  |  | yes |
| Exit | `Exit` |  | `Cmd+Q` | yes |
| Invalidate Caches… | `InvalidateCaches` |  |  | — |

### View

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| Quick Switch Scheme… | `QuickChangeScheme` | `` Ctrl+` `` | `` Ctrl+` `` | yes |
| Theme: Toggle Dark/Light | `ChangeLaf` |  |  | yes |
| Toggle Full Screen Mode | `ToggleFullScreen` |  | `Ctrl+Cmd+F` | — |
| Toggle Presentation Mode | `TogglePresentationMode` |  |  | — |
| Toggle Distraction Free Mode | `ToggleDistractionFreeMode` |  |  | — |
| Toggle Zen Mode | `ToggleZenMode` |  |  | — |
| Jump to Navigation Bar | `ShowNavBar` | `Alt+Home` | `Cmd+Up` | — |
| Quick Documentation | `QuickJavaDoc` | `Ctrl+Q` | `F1`, `Ctrl+J` | — |
| External Documentation | `ExternalJavaDoc` | `Shift+F1` | `Shift+F1` | — |
| Quick Definition | `QuickImplementations` | `Ctrl+Shift+I` | `Alt+Space`, `Cmd+Y` | — |
| Parameter Info | `ParameterInfo` | `Ctrl+P` | `Cmd+P` | yes |
| Type Info | `ExpressionTypeInfo` | `Ctrl+Shift+P` | `Ctrl+Shift+P` | — |
| Context Info | `EditorContextInfo` | `Alt+Q` | `Ctrl+Shift+Q` | — |
| Error Description | `ShowErrorDescription` | `Ctrl+F1` | `Cmd+F1` | — |
| Recent Files | `RecentFiles` | `Ctrl+E` | `Cmd+E` | yes |
| Recently Changed Files | `RecentChangedFiles` |  |  | yes |
| Recent Locations | `RecentLocations` | `Ctrl+Shift+E` | `Cmd+Shift+E` | — |
| Switcher | `Switcher` | `Ctrl+Tab`, `Ctrl+Shift+Tab` | `Ctrl+Tab`, `Ctrl+Shift+Tab` | yes |
| Toggle Rendered View | `ToggleRenderedDocPresentation` | `Ctrl+Alt+Q` | `Ctrl+Alt+Q` | — |
| Compare Files | `CompareTwoFiles` | `Ctrl+D` | `Cmd+D` | — |
| Inlay Hints | `ViewInplaceComments` |  |  | — |

### File > File Properties

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| File Path | `ShowFilePath` | `Ctrl+Alt+F12` | `Cmd+Alt+F12` | — |

### View > Appearance

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| Show Main Menu in Separate Toolbar | `ToggleMainMenuInSeparateToolbar` |  |  | yes |
| Compact Mode | `ToggleCompactMode` |  |  | yes |
| Tool Window Bars | `ViewToolButtons` |  |  | yes |

### File > Manage IDE Settings

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| Export Settings… | `ExportSettings` |  |  | — |

### Edit

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| Undo | `$Undo` | `Ctrl+Z`, `Alt+Backspace` | `Cmd+Z` | native |
| Redo | `$Redo` | `Ctrl+Shift+Z`, `Alt+Shift+Backspace` | `Cmd+Shift+Z` | native |
| Cut | `$Cut` | `Ctrl+X`, `Shift+Delete` | `Cmd+X` | native |
| Copy | `$Copy` | `Ctrl+C`, `Ctrl+Insert` | `Cmd+C` | native |
| Paste | `$Paste` | `Ctrl+V`, `Shift+Insert` | `Cmd+V` | native |
| Paste from History… | `PasteMultiple` | `Ctrl+Shift+V` | `Cmd+Shift+V` | — |
| Paste as Plain Text | `PasteSimple` | `Ctrl+Alt+Shift+V` | `Cmd+Alt+Shift+V` | yes |
| Select All | `$SelectAll` | `Ctrl+A` | `Cmd+A` | native |
| Delete | `$Delete` | `Delete` | `Delete`, `Backspace` | native |
| Column Selection Mode | `EditorToggleColumnMode` | `Alt+Shift+Insert` | `Cmd+Shift+8` | — |
| Add Selection for Next Occurrence | `SelectNextOccurrence` | `Alt+J` | `Ctrl+G` | yes |
| Unselect Occurrence | `UnselectPreviousOccurrence` | `Alt+Shift+J` | `Ctrl+Shift+G` | yes |
| Select All Occurrences | `SelectAllOccurrences` | `Ctrl+Alt+Shift+J` | `Ctrl+Cmd+G` | yes |
| Extend Selection | `EditorSelectWord` | `Ctrl+W` | `Alt+Up` | yes |
| Shrink Selection | `EditorUnSelectWord` | `Ctrl+Shift+W` | `Alt+Down` | yes |
| Join Lines | `EditorJoinLines` | `Ctrl+Shift+J` | `Ctrl+Shift+J` | yes |
| Duplicate Line or Selection | `EditorDuplicate` | `Ctrl+D` | `Cmd+D` | yes |
| Delete Line | `EditorDeleteLine` | `Ctrl+Y` | `Cmd+Backspace` | yes |
| Toggle Case | `EditorToggleCase` | `Ctrl+Shift+U` | `Cmd+Shift+U` | yes |
| Sort Lines | `EditorSortLines` |  |  | — |
| Reverse Lines | `EditorReverseLines` |  |  | — |
| Fill Paragraph | `FillParagraph` |  |  | — |

### Edit > Copy Path > Reference

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| Copy Absolute Path | `CopyPaths` | `Ctrl+Shift+C` | `Cmd+Shift+C` | yes |
| Copy Reference | `CopyReference` | `Ctrl+Alt+Shift+C` | `Cmd+Alt+Shift+C` | yes |

### Edit > Find

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| Find… | `Find` | `Ctrl+F` | `Cmd+F` | yes |
| Replace… | `Replace` | `Ctrl+R` | `Cmd+R` | yes |
| Find Next / Move to Next Occurrence | `FindNext` | `F3` | `Cmd+G` | yes |
| Find Previous / Move to Previous Occurrence | `FindPrevious` | `Shift+F3` | `Cmd+Shift+G` | yes |
| Find Word at Caret | `FindWordAtCaret` | `Ctrl+F3` | `Alt+Cmd+F3` | — |
| Find in Files… | `FindInPath` | `Ctrl+Shift+F` | `Cmd+Shift+F` | yes |
| Replace in Files… | `ReplaceInPath` | `Ctrl+Shift+R` | `Cmd+Shift+R` | — |
| Find Usages | `FindUsages` | `Alt+F7` | `Alt+F7` | — |
| Find Usages in File | `FindUsagesInFile` | `Ctrl+F7` | `Cmd+F7` | — |
| Highlight Usages in File | `HighlightUsagesInFile` | `Ctrl+Shift+F7` | `Cmd+Shift+F7` | — |
| Show Usages | `ShowUsages` | `Ctrl+Alt+F7` | `Cmd+Alt+F7` | — |
| Find Usages Settings… | `ShowSettingsAndFindUsages` | `Ctrl+Alt+Shift+F7` | `Cmd+Alt+Shift+F7` | — |
| Search Structurally… | `StructuralSearchPlugin.StructuralSearchAction` |  |  | — |

### View > Tool Windows

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| Project | `ActivateProjectToolWindow` | `Alt+1` | `Cmd+1` | yes |
| Bookmarks | `ActivateBookmarksToolWindow` | `Alt+2` | `Cmd+2` | — |
| Find | `ActivateFindToolWindow` | `Alt+3` | `Cmd+3` | — |
| Run | `ActivateRunToolWindow` | `Alt+4` | `Cmd+4` | yes |
| Debug | `ActivateDebugToolWindow` | `Alt+5` | `Cmd+5` | yes |
| Problems | `ActivateProblemsViewToolWindow` | `Alt+6` | `Cmd+6` | yes |
| Structure | `ActivateStructureToolWindow` | `Alt+7` | `Cmd+7` | yes |
| Services | `ActivateServicesToolWindow` | `Alt+8` | `Cmd+8` | yes |
| Git | `ActivateVersionControlToolWindow` | `Alt+9` | `Cmd+9` | yes |
| Commit | `ActivateCommitToolWindow` | `Alt+0` | `Cmd+0` | yes |
| Terminal | `ActivateTerminalToolWindow` | `Alt+F12` | `Alt+F12` | yes |
| Database | `ActivateDatabaseToolWindow` |  |  | yes |
| Notifications | `ActivateNotificationsToolWindow` |  |  | — |
| AI Chat | `ActivateAIChatToolWindow` |  |  | yes |
| Pull Requests | `ActivatePullRequestsToolWindow` |  |  | yes |
| TODO | `ActivateTODOToolWindow` |  |  | — |

### Navigate > Bookmarks

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| Toggle Bookmark | `ToggleBookmark` | `F11` | `F3` | — |
| Toggle Bookmark with Mnemonic | `ToggleBookmarkWithMnemonic` | `Ctrl+F11` | `Alt+F3` | — |
| Show All Bookmarks | `ShowBookmarks` | `Shift+F11` | `Cmd+F3` | — |
| Go to Bookmark 0 | `GotoBookmark0` | `Ctrl+0` | `Ctrl+0` | — |
| Go to Bookmark 1 | `GotoBookmark1` | `Ctrl+1` | `Ctrl+1` | — |
| Go to Bookmark 2 | `GotoBookmark2` | `Ctrl+2` | `Ctrl+2` | — |
| Go to Bookmark 3 | `GotoBookmark3` | `Ctrl+3` | `Ctrl+3` | — |
| Go to Bookmark 4 | `GotoBookmark4` | `Ctrl+4` | `Ctrl+4` | — |
| Go to Bookmark 5 | `GotoBookmark5` | `Ctrl+5` | `Ctrl+5` | — |
| Go to Bookmark 6 | `GotoBookmark6` | `Ctrl+6` | `Ctrl+6` | — |
| Go to Bookmark 7 | `GotoBookmark7` | `Ctrl+7` | `Ctrl+7` | — |
| Go to Bookmark 8 | `GotoBookmark8` | `Ctrl+8` | `Ctrl+8` | — |
| Go to Bookmark 9 | `GotoBookmark9` | `Ctrl+9` | `Ctrl+9` | — |
| Toggle Bookmark 0 | `ToggleBookmark0` | `Ctrl+Shift+0` | `Ctrl+Shift+0` | — |
| Toggle Bookmark 1 | `ToggleBookmark1` | `Ctrl+Shift+1` | `Ctrl+Shift+1` | — |
| Toggle Bookmark 2 | `ToggleBookmark2` | `Ctrl+Shift+2` | `Ctrl+Shift+2` | — |
| Toggle Bookmark 3 | `ToggleBookmark3` | `Ctrl+Shift+3` | `Ctrl+Shift+3` | — |
| Toggle Bookmark 4 | `ToggleBookmark4` | `Ctrl+Shift+4` | `Ctrl+Shift+4` | — |
| Toggle Bookmark 5 | `ToggleBookmark5` | `Ctrl+Shift+5` | `Ctrl+Shift+5` | — |
| Toggle Bookmark 6 | `ToggleBookmark6` | `Ctrl+Shift+6` | `Ctrl+Shift+6` | — |
| Toggle Bookmark 7 | `ToggleBookmark7` | `Ctrl+Shift+7` | `Ctrl+Shift+7` | — |
| Toggle Bookmark 8 | `ToggleBookmark8` | `Ctrl+Shift+8` | `Ctrl+Shift+8` | — |
| Toggle Bookmark 9 | `ToggleBookmark9` | `Ctrl+Shift+9` | `Ctrl+Shift+9` | — |

### Code > Folding

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| Expand | `ExpandRegion` | `Ctrl+=`, `Ctrl+NumPad+` | `Cmd+=`, `Cmd+NumPad+` | yes |
| Collapse | `CollapseRegion` | `Ctrl+-`, `Ctrl+NumPad-` | `Cmd+-`, `Cmd+NumPad-` | yes |
| Expand Recursively | `ExpandRegionRecursively` | `Ctrl+Alt+=`, `Ctrl+Alt+NumPad+` | `Cmd+Alt+=`, `Cmd+Alt+NumPad+` | yes |
| Collapse Recursively | `CollapseRegionRecursively` | `Ctrl+Alt+-`, `Ctrl+Alt+NumPad-` | `Cmd+Alt+-`, `Cmd+Alt+NumPad-` | yes |
| Expand All | `ExpandAllRegions` | `Ctrl+Shift+=`, `Ctrl+Shift+NumPad+` | `Cmd+Shift+=`, `Cmd+Shift+NumPad+` | yes |
| Collapse All | `CollapseAllRegions` | `Ctrl+Shift+-`, `Ctrl+Shift+NumPad-` | `Cmd+Shift+-`, `Cmd+Shift+NumPad-` | yes |
| Fold Selection / Remove Region | `CollapseSelection` | `Ctrl+.` | `Cmd+.` | — |
| Collapse Doc Comments | `CollapseDocComments` |  |  | — |

### Refactor

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| Refactor This… | `Refactorings.QuickListPopupAction` | `Ctrl+Alt+Shift+T` | `Ctrl+T` | — |
| Rename… | `RenameElement` | `Shift+F6` | `Shift+F6` | — |
| Rename File… | `RenameFile` |  |  | — |
| Change Signature… | `ChangeSignature` | `Ctrl+F6` | `Cmd+F6` | — |
| Move… | `Move` | `F6` | `F6` | — |
| Copy File… | `CopyElement` | `F5` | `F5` | — |
| Safe Delete… | `SafeDelete` | `Alt+Delete` | `Cmd+Delete` | — |
| Inline… | `Inline` | `Ctrl+Alt+N` | `Cmd+Alt+N` | — |
| Push Members Down… | `MemberPushDown` |  |  | — |
| Pull Members Up… | `MembersPullUp` |  |  | — |

### Refactor > Extract > Introduce

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| Extract Method… | `ExtractMethod` | `Ctrl+Alt+M` | `Cmd+Alt+M` | — |
| Introduce Variable… | `IntroduceVariable` | `Ctrl+Alt+V` | `Cmd+Alt+V` | — |
| Introduce Field… | `IntroduceField` | `Ctrl+Alt+F` | `Cmd+Alt+F` | — |
| Introduce Constant… | `IntroduceConstant` | `Ctrl+Alt+C` | `Cmd+Alt+C` | — |
| Introduce Parameter… | `IntroduceParameter` | `Ctrl+Alt+P` | `Cmd+Alt+P` | — |
| Extract Interface… | `ExtractInterface` |  |  | — |
| Extract Superclass… | `ExtractSuperclass` |  |  | — |

### Run

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| Run | `Run` | `Shift+F10` | `Ctrl+R` | yes |
| Debug | `Debug` | `Shift+F9` | `Ctrl+D` | yes |
| Run… | `ChooseRunConfiguration` | `Alt+Shift+F10` | `Ctrl+Alt+R` | yes |
| Debug… | `ChooseDebugConfiguration` | `Alt+Shift+F9` | `Ctrl+Alt+D` | yes |
| Run Context Configuration | `RunClass` | `Ctrl+Shift+F10` | `Ctrl+Shift+R` | yes |
| Debug Context Configuration | `DebugClass` | `Ctrl+Shift+F9` | `Ctrl+Shift+D` | yes |
| Edit Configurations… | `editRunConfigurations` |  |  | yes |
| Stop | `Stop` | `Ctrl+F2` | `Cmd+F2` | yes |
| Rerun | `Rerun` | `Ctrl+F5` | `Cmd+R` | yes |
| View Breakpoints… | `ViewBreakpoints` | `Ctrl+Shift+F8` | `Cmd+Shift+F8` | yes |
| Attach to Process… | `XDebugger.AttachToProcess` | `Ctrl+Alt+F5` | `Cmd+Alt+F5` | — |

### Build

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| Build Project | `CompileDirty` | `Ctrl+F9` | `Cmd+F9` | — |

### Run > Toggle Breakpoint

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| Toggle Line Breakpoint | `ToggleLineBreakpoint` | `Ctrl+F8` | `Cmd+F8` | yes |

### Git

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| Commit… | `CheckinProject` | `Ctrl+K` | `Cmd+K` | yes |
| Push… | `Vcs.Push` | `Ctrl+Shift+K` | `Cmd+Shift+K` | yes |
| Update Project… | `Vcs.UpdateProject` | `Ctrl+T` | `Cmd+T` | yes |
| Commit and Push… | `Git.Commit.And.Push.Executor` | `Ctrl+Alt+K` | `Cmd+Alt+K` | — |
| VCS Operations Popup… | `Vcs.QuickListPopupAction` | `` Alt+` `` | `Ctrl+V` | — |
| Branches… | `Git.Branches` |  |  | yes |
| Fetch | `Git.Fetch` |  |  | yes |
| Show Git Log | `Vcs.Show.Log` |  |  | yes |
| Pull… | `Git.Pull` |  |  | — |
| Stash Changes… | `Git.Stash` |  |  | — |
| Unstash Changes… | `Git.Unstash` |  |  | — |
| New Branch… | `Git.CreateNewBranch` |  |  | yes |
| Rollback… | `ChangesView.Rollback` | `Ctrl+Alt+Z` | `Cmd+Alt+Z` | — |
| Show Commit Message History | `Vcs.ShowMessageHistory` | `Ctrl+M` | `Cmd+M` | — |

### Git > Current File

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| Annotate with Git Blame | `Annotate` |  |  | yes |
| Show History | `Vcs.ShowTabbedFileHistory` |  |  | — |
| Compare with Revision… | `Compare.SameVersion` |  |  | — |
| Show History for Selection | `Vcs.ShowHistoryForBlock` |  |  | — |
| Rollback Lines | `Vcs.RollbackChangedLines` | `Ctrl+Alt+Z` | `Cmd+Alt+Z` | — |

### File > Local History

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| Show Local History | `LocalHistory.ShowHistory` |  |  | — |

### Window

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| Hide All Tool Windows | `HideAllWindows` | `Ctrl+Shift+F12` | `Cmd+Shift+F12` | yes |
| Jump to Last Tool Window | `JumpToLastWindow` | `F12` | `F12` | yes |
| Hide Active Tool Window | `HideActiveWindow` | `Shift+Escape` | `Shift+Escape` | yes |
| Hide Side Tool Windows | `HideSideWindows` |  |  | yes |
| Maximize Tool Window | `MaximizeToolWindow` | `Ctrl+Shift+'` | `Cmd+Shift+'` | — |
| Restore Default Layout | `RestoreDefaultLayout` | `Shift+F12` | `Shift+F12` | — |
| Store Current Layout as Default | `StoreDefaultLayout` |  |  | — |
| Close Active Tool Window Tab | `CloseActiveTab` | `Ctrl+Shift+F4` | `Cmd+Shift+F4` | yes |
| Next Project Window | `NextProjectWindow` | `Ctrl+Alt+]` | `` Cmd+` `` | — |
| Previous Project Window | `PreviousProjectWindow` | `Ctrl+Alt+[` | `` Cmd+Shift+` `` | — |

### Window > Active Tool Window

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| Stretch to Left | `ResizeToolWindowLeft` | `Ctrl+Shift+Left` | `Ctrl+Alt+Shift+Left` | yes |
| Stretch to Right | `ResizeToolWindowRight` | `Ctrl+Shift+Right` | `Ctrl+Alt+Shift+Right` | yes |
| Stretch to Top | `ResizeToolWindowUp` | `Ctrl+Shift+Up` | `Ctrl+Alt+Shift+Up` | yes |
| Stretch to Bottom | `ResizeToolWindowDown` | `Ctrl+Shift+Down` | `Ctrl+Alt+Shift+Down` | yes |

### Window > Editor Tabs

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| Select Next Tab | `NextTab` | `Alt+Right` | `Ctrl+Right`, `Cmd+Shift+]` | yes |
| Select Previous Tab | `PreviousTab` | `Alt+Left` | `Ctrl+Left`, `Cmd+Shift+[` | yes |
| Close Tab | `CloseContent` | `Ctrl+F4` | `Cmd+W` | yes |
| Close All Tabs | `CloseAllEditors` |  |  | yes |
| Close Other Tabs | `CloseAllEditorsButActive` |  | `Cmd+Alt+W` | yes |
| Close Unmodified Tabs | `CloseAllUnmodifiedEditors` |  |  | yes |
| Reopen Closed Tab | `ReopenClosedTab` |  |  | yes |
| Pin Tab | `PinActiveEditorTab` |  |  | — |
| Split Right | `SplitVertically` |  |  | — |
| Split Down | `SplitHorizontally` |  |  | — |
| Goto Next Splitter | `NextSplitter` |  | `Alt+Tab` | — |
| Goto Previous Splitter | `PrevSplitter` |  | `Alt+Shift+Tab` | — |

## Editor Actions

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| Split Line | `EditorSplitLine` | `Ctrl+Enter` | `Cmd+Enter` | yes |
| Start New Line | `EditorStartNewLine` | `Shift+Enter` | `Shift+Enter` | yes |
| Start New Line Before Current | `EditorStartNewLineBefore` | `Ctrl+Alt+Enter` | `Cmd+Alt+Enter` | yes |
| Scroll Up | `EditorScrollUp` | `Ctrl+Up` |  | yes |
| Scroll Down | `EditorScrollDown` | `Ctrl+Down` |  | yes |
| Delete to Word Start | `EditorDeleteToWordStart` | `Ctrl+Backspace` | `Alt+Backspace` | native |
| Delete to Word End | `EditorDeleteToWordEnd` | `Ctrl+Delete` | `Alt+Delete` | native |
| Delete to Line Start | `EditorDeleteToLineStart` |  |  | native |
| Delete to Line End | `EditorDeleteToLineEnd` |  | `Ctrl+K` | native |
| Indent Selection | `EditorIndentSelection` | `Tab` | `Tab` | native |
| Unindent Line or Selection | `EditorUnindentSelection` | `Shift+Tab` | `Shift+Tab` | native |
| Enter | `EditorEnter` | `Enter` | `Enter` | native |
| Escape | `EditorEscape` | `Escape` | `Escape` | native |
| Backspace | `EditorBackSpace` | `Backspace`, `Shift+Backspace` | `Backspace`, `Shift+Backspace` | native |
| Move Caret to Line Start | `EditorLineStart` | `Home` | `Home`, `Cmd+Left` | native |
| Move Caret to Line End | `EditorLineEnd` | `End` | `End`, `Cmd+Right` | native |
| Move Caret to Text Start | `EditorTextStart` | `Ctrl+Home` | `Cmd+Home` | native |
| Move Caret to Text End | `EditorTextEnd` | `Ctrl+End` | `Cmd+End` | native |
| Move Caret to Previous Word | `EditorPreviousWord` | `Ctrl+Left` | `Alt+Left` | native |
| Move Caret to Next Word | `EditorNextWord` | `Ctrl+Right` | `Alt+Right` | native |
| Move Caret to Previous Word with Selection | `EditorPreviousWordWithSelection` | `Ctrl+Shift+Left` | `Alt+Shift+Left` | native |
| Move Caret to Next Word with Selection | `EditorNextWordWithSelection` | `Ctrl+Shift+Right` | `Alt+Shift+Right` | native |
| Page Up | `EditorPageUp` | `PageUp` | `PageUp` | native |
| Page Down | `EditorPageDown` | `PageDown` | `PageDown` | native |
| Move Caret to Page Top | `EditorMoveToPageTop` | `Ctrl+PageUp` | `Cmd+PageUp` | native |
| Move Caret to Page Bottom | `EditorMoveToPageBottom` | `Ctrl+PageDown` | `Cmd+PageDown` | native |
| Clone Caret Above | `EditorCloneCaretAbove` |  |  | — |
| Clone Caret Below | `EditorCloneCaretBelow` |  |  | — |
| Toggle Insert/Overwrite | `EditorToggleInsertState` | `Insert` |  | native |
| Choose Lookup Item and Replace | `EditorChooseLookupItemReplace` | `Tab` | `Tab` | native |

## Debugger Actions

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| Mute Breakpoints | `XDebugger.MuteBreakpoints` |  |  | yes |
| Step Over | `StepOver` | `F8` | `F8` | yes |
| Step Into | `StepInto` | `F7` | `F7` | yes |
| Smart Step Into | `SmartStepInto` | `Shift+F7` | `Shift+F7` | yes |
| Force Step Into | `ForceStepInto` | `Alt+Shift+F7` | `Alt+Shift+F7` | yes |
| Step Out | `StepOut` | `Shift+F8` | `Shift+F8` | yes |
| Run to Cursor | `RunToCursor` | `Alt+F9` | `Alt+F9` | yes |
| Force Run to Cursor | `ForceRunToCursor` | `Ctrl+Alt+F9` | `Cmd+Alt+F9` | yes |
| Resume Program | `Resume` | `F9` | `Cmd+Alt+R` | yes |
| Pause Program | `Pause` |  |  | yes |
| Evaluate Expression… | `EvaluateExpression` | `Alt+F8` | `Alt+F8` | yes |
| Quick Evaluate Expression | `QuickEvaluateExpression` | `Ctrl+Alt+F8` | `Cmd+Alt+F8` | yes |
| Show Execution Point | `ShowExecutionPoint` | `Alt+F10` | `Alt+F10` | yes |
| Add to Watches | `Debugger.AddToWatch` |  |  | yes |

## Other

| Action | Action id | Windows / Linux | macOS | In Forge |
| --- | --- | --- | --- | --- |
| Show Context Menu | `ShowPopupMenu` | `ContextMenu` |  | yes |
| Main Menu | `MainMenuButton.ShowMenu` | `Alt+\` |  | yes |

¹ `Ctrl+O` and `Ctrl+I` open Override/Implement Methods in PHP editors through the PHP pack's
editor keys; elsewhere they are not available yet.

## Plugins

Actions that Forge's plugins add (group **Plugins** in Settings | Keymap, some under Main Menu).
"Context" is where the shortcut applies.

| Action | Action id | Windows / Linux | macOS | Context |
| --- | --- | --- | --- | --- |
| Run Artisan Command… | `Laravel.RunArtisan` | `Ctrl+Shift+X` | `Cmd+Shift+X` | any |
| Routes (Laravel) | `Laravel.ShowRoutes` |  |  | any |
| Run Console Command… (Symfony) | `Symfony.RunConsoleCommand` |  |  | any |
| Symfony Routes | `Symfony.ShowRoutes` |  |  | any |
| Doctrine Entities | `Doctrine.ShowEntities` |  |  | any |
| npm (tool window) | `ActivatenpmToolWindow` |  |  | any |
| Reload npm Scripts | `npm.reload` |  |  | any |
| Install Dependencies | `npm.install` |  |  | any |
| New Terminal Tab | `Terminal.NewTab` | `Ctrl+Shift+T` | `Cmd+T` | tool window |
| Close Terminal Tab | `Terminal.CloseTab` |  |  | tool window |
| Start Listening for PHP Debug Connections | `PhpListenDebugAction` |  |  | any |
| New Scratch Buffer | `NewScratchBuffer` |  |  | any |
| Execute (database console) | `Console.Jdbc.Execute` | `Ctrl+Enter` | `Cmd+Enter` | editor |
| Submit (data grid) | `Console.TableResult.SubmitChanges` | `Ctrl+Enter` | `Cmd+Enter` | tool window |
| Add Row | `Console.TableResult.AddRow` | `Alt+Insert` | `Cmd+N` | tool window |
| Delete Rows | `Console.TableResult.DeleteRows` | `Ctrl+Y` | `Cmd+Backspace` | tool window |
| Revert Selected | `Console.TableResult.Revert` | `Ctrl+Alt+Z` | `Cmd+Alt+Z` | tool window |
| Reload Page | `Console.TableResult.Reload` |  |  | any |
| Jump to Query Console… | `Database.OpenConsole` | `Ctrl+Shift+F10` | `Cmd+Shift+F10` | tool window |
| Data Source Properties | `Database.Properties` | `Shift+Enter` | `Shift+Enter` | tool window |
| New Data Source… | `Database.NewDataSource` |  |  | any |

## Commands without shortcuts

Forge also registers commands that are not PHPStorm actions. They have no default shortcut but
appear in Find Action and can be bound in Settings | Keymap, for example: Show Git Log
(`git.log`), Fetch (`git.fetch`), New Branch… (`git.newBranch`), Show Diff (`git.showDiff`),
Create Merge Request… (`forges.createMergeRequest`), View Pull/Merge Requests
(`forges.showMergeRequests`), Refresh Pull/Merge Requests (`forges.refresh`), Detect Project
Type (`project.redetect`), Reindex PHP Files (`php.reindex`), Generate PHPDoc Comment
(`php.generatePhpDoc`), Reindex Lua Files (`lua.reindex`), Refresh Symfony Index
(`symfony.refresh`), Refresh Doctrine Entities (`doctrine.refresh`), Refresh Services
(`Services.Refresh`), Pull Image… (`Docker.PullImage`), New SSH Connection…
(`Remote.NewSshConnection`), Reconnect to Remote Host (`Remote.Reconnect`), SSH
Configurations… (`remote.settings`), Language Servers… (`langservers.settings`), AI Providers…
(`ai.settings`), Open AI Chat (`AIChat.Focus`), New AI Chat (`AIChat.NewChat`), Browser
Companion Settings (`BrowserCompanion.Settings`), Keymap Settings (`settings.keymap`).
