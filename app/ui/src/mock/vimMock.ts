/**
 * Browser-mode `~/.ideavimrc`: a small example configuration instead of reading the home
 * directory.
 */
import { registerMock } from "@/ipc";
import type { VimRc } from "@/ipc/vim";

export const DEMO_IDEAVIMRC = `" Forge reads ~/.ideavimrc like IdeaVim does.
let mapleader = " "
set hlsearch incsearch ignorecase smartcase
set scrolloff=3

" IDE actions from Vim
nmap <leader>r <Action>(RenameElement)
nmap <leader>f :action ReformatCode<CR>
nmap <leader>e :action SearchEverywhere<CR>
`;

registerMock("vim_read_rc", (args): VimRc => {
  const path = (args as { path?: string | null } | undefined)?.path;
  return { path: path || "~/.ideavimrc", text: DEMO_IDEAVIMRC };
});
