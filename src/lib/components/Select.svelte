<script lang="ts">
  import { onMount } from 'svelte';

  interface Option {
    value: string;
    label: string;
  }

  let {
    value = $bindable(),
    options = [],
    id = '',
    ariaLabel = 'Select option'
  }: {
    value: string;
    options: Option[];
    id?: string;
    ariaLabel?: string;
  } = $props();

  let isOpen = $state(false);

  const toggleDropdown = (event: MouseEvent) => {
    event.stopPropagation();
    isOpen = !isOpen;
  };

  const selectOption = (val: string) => {
    value = val;
    isOpen = false;
  };

  const getSelectedLabel = () => {
    const selected = options.find(opt => opt.value === value);
    return selected ? selected.label : '';
  };

  onMount(() => {
    const handleGlobalClick = () => {
      isOpen = false;
    };
    window.addEventListener('click', handleGlobalClick);
    return () => {
      window.removeEventListener('click', handleGlobalClick);
    };
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="custom-select-container" onclick={(event) => event.stopPropagation()}>
  <button
    {id}
    class="custom-select-trigger"
    type="button"
    aria-haspopup="listbox"
    aria-expanded={isOpen}
    onclick={toggleDropdown}
  >
    <span class="selected-text">{getSelectedLabel()}</span>
    <svg class="chevron-icon" class:is-open={isOpen} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
      <polyline points="6 9 12 15 18 9" />
    </svg>
  </button>

  {#if isOpen}
    <div
      class="custom-select-options"
      role="listbox"
      aria-label={ariaLabel}
    >
      {#each options as option (option.value)}
        <button
          class="custom-select-option"
          class:is-selected={value === option.value}
          role="option"
          aria-selected={value === option.value}
          type="button"
          onclick={() => selectOption(option.value)}
        >
          {option.label}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .custom-select-container {
    position: relative;
    width: 140px;
  }

  .custom-select-trigger {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    height: 30px;
    padding: 0 10px;
    border-radius: 7px;
    border: 1px solid var(--border);
    background: var(--row-bg);
    color: var(--text-color);
    font: inherit;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition: border-color 0.15s ease, background-color 0.15s ease, box-shadow 0.15s ease;
    box-sizing: border-box;
    text-align: left;
    outline: none;
  }

  .custom-select-trigger:hover {
    border-color: var(--close-btn-hover-border);
    background: var(--drop-hover-bg);
  }

  .custom-select-trigger:focus-visible {
    border-color: #3b82f6;
    box-shadow: 0 0 0 2px rgba(59, 130, 246, 0.15);
  }

  :global(html.dark) .custom-select-trigger:focus-visible {
    border-color: #60a5fa;
    box-shadow: 0 0 0 2px rgba(96, 165, 250, 0.2);
  }

  .chevron-icon {
    width: 10px;
    height: 10px;
    color: var(--text-muted);
    transition: transform 0.2s ease;
    flex-shrink: 0;
  }

  .chevron-icon.is-open {
    transform: rotate(180deg);
  }

  .custom-select-options {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    width: 100%;
    background: var(--panel-bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.08);
    z-index: 100;
    padding: 4px;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 2px;
    animation: dropdown-fade-in 0.15s cubic-bezier(0.16, 1, 0.3, 1);
  }

  :global(html.dark) .custom-select-options {
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.3);
  }

  @keyframes dropdown-fade-in {
    0% {
      opacity: 0;
      transform: translateY(-4px) scale(0.98);
    }
    100% {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  .custom-select-option {
    width: 100%;
    height: 28px;
    padding: 0 8px;
    border: none;
    border-radius: 5px;
    background: transparent;
    color: var(--text-color);
    font: inherit;
    font-size: 12px;
    font-weight: 500;
    text-align: left;
    cursor: pointer;
    display: flex;
    align-items: center;
    transition: background-color 0.12s ease, color 0.12s ease;
    box-sizing: border-box;
    outline: none;
  }

  .custom-select-option:hover {
    background: var(--drop-hover-bg);
  }

  .custom-select-option.is-selected {
    background: rgba(59, 130, 246, 0.08);
    color: #3b82f6;
    font-weight: 600;
  }

  :global(html.dark) .custom-select-option.is-selected {
    background: rgba(96, 165, 250, 0.12);
    color: #60a5fa;
  }
</style>
