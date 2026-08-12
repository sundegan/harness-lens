<script lang="ts">
import ChevronLeftIcon from '@lucide/svelte/icons/chevron-left';
import { Pagination as PaginationPrimitive } from 'bits-ui';
import { buttonVariants } from '$lib/components/ui/button/index.js';
import { cn } from '$lib/utils.js';

let {
  ref = $bindable(null),
  class: className,
  label = 'Previous',
  'aria-label': ariaLabel = label,
  ...restProps
}: PaginationPrimitive.PrevButtonProps & { label?: string } = $props();
</script>

<PaginationPrimitive.PrevButton
	bind:ref
	aria-label={ariaLabel}
	class={cn(
		buttonVariants({ variant: "ghost", size: "default" }),
		"pl-1.5!",
		className
	)}
	{...restProps}
>
	<ChevronLeftIcon data-icon="inline-start" />
	<span class="hidden sm:block">{label}</span>
</PaginationPrimitive.PrevButton>
