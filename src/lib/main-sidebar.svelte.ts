import { createContext } from 'svelte';

export class MainSidebarState {
  open = $state(true);
  isToggling = $state(false);
  #toggleGeneration = 0;

  toggle() {
    const generation = ++this.#toggleGeneration;
    this.isToggling = true;
    this.open = !this.open;

    if (typeof window === 'undefined') {
      this.isToggling = false;
      return;
    }

    // Keep the layout transition disabled through the state update and the
    // first layout commit. This makes button toggles follow the same geometry
    // path as pointer resizing instead of exposing an intermediate wide track
    // with collapsed menu content.
    window.requestAnimationFrame(() => {
      window.requestAnimationFrame(() => {
        if (generation === this.#toggleGeneration) this.isToggling = false;
      });
    });
  }
}

export const [getMainSidebar, setMainSidebar] = createContext<MainSidebarState>();
