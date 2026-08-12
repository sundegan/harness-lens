<script lang="ts">
import { Button } from '$lib/components/ui/button';
import { Input } from '$lib/components/ui/input';
import * as Pagination from '$lib/components/ui/pagination';
import * as Select from '$lib/components/ui/select';
import { i18nManager } from '$lib/i18n.svelte';
import { normalizePageInput } from '$lib/pagination';

let {
  page,
  pageSize,
  totalCount,
  totalLabel,
  idPrefix,
  loading = false,
  pageSizeOptions = [25, 50, 100],
  onPageChange,
  onPageSizeChange,
}: {
  page: number;
  pageSize: number;
  totalCount: number;
  totalLabel: string;
  idPrefix: string;
  loading?: boolean;
  pageSizeOptions?: number[];
  onPageChange: (page: number) => void;
  onPageSizeChange: (pageSize: number) => void;
} = $props();

let pageInput = $state('');
const pageSizeId = $derived(`${idPrefix}-page-size`);
const pageSizeLabelId = $derived(`${idPrefix}-page-size-label`);
const pageCount = $derived(Math.max(1, Math.ceil(totalCount / pageSize)));

function changePage(value: number) {
  const nextPage = Math.min(Math.max(value, 1), pageCount);
  if (nextPage !== page) onPageChange(nextPage);
}

function changePageSize(value: string) {
  const nextPageSize = Number(value);
  if (Number.isSafeInteger(nextPageSize) && pageSizeOptions.includes(nextPageSize)) {
    onPageSizeChange(nextPageSize);
  }
}

function submitPage(event: SubmitEvent) {
  event.preventDefault();
  const nextPage = normalizePageInput(pageInput, pageCount);
  if (nextPage === null) return;
  pageInput = '';
  changePage(nextPage);
}
</script>

<footer class="flex flex-col gap-3 border-t bg-muted/10 px-5 py-3 text-xs text-muted-foreground xl:flex-row xl:items-center xl:justify-between">
  <p class="shrink-0">{totalLabel}</p>

  <div class="flex min-w-0 flex-wrap items-center gap-2">
    <div class="flex items-center gap-2 whitespace-nowrap">
      <span id={pageSizeLabelId}>{i18nManager.t('pagination.rows_per_page')}</span>
      <Select.Root
        type="single"
        value={String(pageSize)}
        onValueChange={changePageSize}
        disabled={loading}
      >
        <Select.Trigger
          id={pageSizeId}
          aria-labelledby={pageSizeLabelId}
          size="sm"
          class="min-w-18 bg-background"
        >
          <span>{pageSize}</span>
        </Select.Trigger>
        <Select.Content>
          <Select.Group>
            {#each pageSizeOptions as option (option)}
              <Select.Item value={String(option)} label={String(option)} />
            {/each}
          </Select.Group>
        </Select.Content>
      </Select.Root>
    </div>

    <Pagination.Root
      count={totalCount}
      perPage={pageSize}
      {page}
      siblingCount={1}
      aria-label={i18nManager.t('pagination.navigation')}
      class="mx-0 w-auto"
      onPageChange={onPageChange}
    >
      {#snippet children({ pages, currentPage })}
        <Pagination.Content>
          <Pagination.Item>
            <Pagination.Previous disabled={loading} label={i18nManager.t('pagination.previous')} />
          </Pagination.Item>
          {#each pages as paginationPage (paginationPage.key)}
            {#if paginationPage.type === 'ellipsis'}
              <Pagination.Item>
                <Pagination.Ellipsis />
              </Pagination.Item>
            {:else}
              <Pagination.Item>
                <Pagination.Link
                  page={paginationPage}
                  isActive={currentPage === paginationPage.value}
                  size="icon-sm"
                  disabled={loading}
                >
                  {paginationPage.value}
                </Pagination.Link>
              </Pagination.Item>
            {/if}
          {/each}
          <Pagination.Item>
            <Pagination.Next disabled={loading} label={i18nManager.t('pagination.next')} />
          </Pagination.Item>
        </Pagination.Content>
      {/snippet}
    </Pagination.Root>

    <form class="flex items-center gap-1.5" onsubmit={submitPage}>
      <Input
        type="number"
        min="1"
        max={pageCount}
        step="1"
        inputmode="numeric"
        value={pageInput}
        disabled={loading}
        class="h-7 w-16 rounded-lg bg-background px-2 text-center text-xs"
        aria-label={i18nManager.t('pagination.page_input')}
        placeholder={String(page)}
        oninput={(event) => (pageInput = event.currentTarget.value)}
      />
      <Button type="submit" variant="outline" size="sm" disabled={loading}>
        {i18nManager.t('pagination.go')}
      </Button>
    </form>
  </div>
</footer>
