/**
 * Menu de contexto global — uma instância só, montada uma vez em `App.svelte`.
 * Qualquer componente pede `contextMenu.open(event, items)`; nenhum precisa
 * saber posicionar ou fechar o próprio menu.
 */

export interface MenuItem {
  label: string;
  action: () => void;
  disabled?: boolean;
}

class ContextMenuState {
  isOpen = $state(false);
  x = $state(0);
  y = $state(0);
  items = $state<MenuItem[]>([]);

  open(event: MouseEvent, items: MenuItem[]) {
    // Tanto o clique direito (contextmenu) quanto o botão "⋯" (click) chegam
    // aqui: sempre bloqueia o menu nativo do webview e nunca deixa o clique
    // borbulhar até o listener da janela que fecha o menu.
    event.preventDefault();
    event.stopPropagation();
    this.items = items;
    this.x = event.clientX;
    this.y = event.clientY;
    this.isOpen = true;
  }

  close() {
    this.isOpen = false;
  }
}

export const contextMenu = new ContextMenuState();
