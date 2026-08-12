<script lang="ts">
import ChevronRightIcon from '@lucide/svelte/icons/chevron-right';
import { Pagination as PaginationPrimitive } from 'bits-ui';
import { buttonVariants } from '$lib/components/ui/button/index.js';
import { cn } from '$lib/utils.js';

let {
  ref = $bindable(null),
  class: className,
  label = 'Next',
  'aria-label': ariaLabel = label,
  ...restProps
}: PaginationPrimitive.NextButtonProps & { label?: string } = $props();
</script>

<PaginationPrimitive.NextButton
	bind:ref
	aria-label={ariaLabel}
	class={cn(buttonVariants({ variant: "ghost", size: "default" }), "pr-1.5!", className)}
	{...restProps}
>
	<span class="hidden sm:block">{label}</span>
	<ChevronRightIcon data-icon="inline-end" />
</PaginationPrimitive.NextButton>
