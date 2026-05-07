<script>
  import {
    marketplaceSearch, marketplaceListInstalled, marketplaceCheckUpdates,
    marketplaceInstall, marketplaceRemove, marketplaceUpdateAll
  } from '../api.js';
  import { Button, Input, Text } from '@plures/design-dojo/primitives';
  import { Box, Tabs } from '@plures/design-dojo/layout';
  import { List, ListItem } from '@plures/design-dojo/data';
  import { Badge } from '@plures/design-dojo';

  /**
   * @typedef {{ id: string, name: string, version: string, description: string, author: string, mean_score: number, review_count: number }} CatalogEntry
   * @typedef {{ id: string, name: string, version: string, description: string, author: string, installed_at: string, last_used: string|null }} InstalledEntry
   * @typedef {{ skill_id: string, installed_version: string, available_version: string }} UpdateEntry
   */

  /** @type {CatalogEntry[]} */
  let searchResults = $state([]);
  /** @type {InstalledEntry[]} */
  let installed = $state([]);
  /** @type {UpdateEntry[]} */
  let availableUpdates = $state([]);

  let searchQuery = $state('');
  let searching = $state(false);
  let searchError = $state('');

  let installing = $state(/** @type {string|null} */ (null));
  let removing = $state(/** @type {string|null} */ (null));
  let updatingAll = $state(false);

  /** @type {'search'|'installed'} */
  let activeSection = $state('search');

  $effect(() => {
    refreshInstalled();
  });

  async function refreshInstalled() {
    try {
      installed = await marketplaceListInstalled();
    } catch {
      installed = [];
    }
    try {
      availableUpdates = await marketplaceCheckUpdates();
    } catch {
      availableUpdates = [];
    }
  }

  async function search() {
    const q = searchQuery.trim();
    if (!q) return;
    searching = true;
    searchError = '';
    try {
      searchResults = await marketplaceSearch(q);
    } catch (err) {
      searchError = String(err);
      searchResults = [];
    } finally {
      searching = false;
    }
  }

  async function install(id) {
    installing = id;
    try {
      await marketplaceInstall(id);
      await refreshInstalled();
    } catch (err) {
      searchError = `Install failed: ${err}`;
    } finally {
      installing = null;
    }
  }

  async function remove(id) {
    removing = id;
    try {
      await marketplaceRemove(id);
      await refreshInstalled();
    } catch (err) {
      searchError = `Remove failed: ${err}`;
    } finally {
      removing = null;
    }
  }

  async function updateAll() {
    updatingAll = true;
    try {
      await marketplaceUpdateAll();
      await refreshInstalled();
    } catch (err) {
      searchError = `Update failed: ${err}`;
    } finally {
      updatingAll = false;
    }
  }

  /** @param {string} id */
  function isInstalled(id) {
    return installed.some(s => s.id === id);
  }

  /** @param {number} score */
  function formatScore(score) {
    return score > 0 ? `★ ${score.toFixed(1)}` : '—';
  }
</script>

<Box border="none" class="marketplace-root">
  <!-- Section toggle -->
  <Box border="none" class="mkt-sections" role="tablist" aria-label="Marketplace sections">
    <Button
      variant={activeSection === 'search' ? 'solid' : 'ghost'}
      size="sm"
      onclick={() => { activeSection = 'search'; }}>
      Discover
    </Button>
    <Button
      variant={activeSection === 'installed' ? 'solid' : 'ghost'}
      size="sm"
      onclick={() => { activeSection = 'installed'; }}>
      Installed
      {#if installed.length > 0}
        <Badge variant="accent" size="sm">{installed.length}</Badge>
      {/if}
    </Button>
  </Box>

  <!-- Discover panel -->
  {#if activeSection === 'search'}
    <Box role="tabpanel" class="mkt-panel">
      <Box border="none" class="mkt-search-row">
        <Input
          bind:value={searchQuery}
          placeholder="Search procedures…"
          onsubmit={search}
        />
        <Button variant="solid" size="sm"
          onclick={search}
          disabled={searching || !searchQuery.trim()}>
          {searching ? 'Searching…' : 'Search'}
        </Button>
      </Box>

      {#if searchError}
        <Text color="error" role="alert">{searchError}</Text>
      {/if}

      {#if searchResults.length === 0 && !searching}
        <Text dim>Enter a query above to discover community procedures.</Text>
      {:else}
        <List aria-label="Search results">
          {#each searchResults as entry (entry.id)}
            <ListItem class="mkt-card">
              <Box border="none" class="mkt-card-header">
                <Text inline bold>{entry.name}</Text>
                <Text inline dim>v{entry.version}</Text>
                <Text inline color="accent" aria-label="Rating">{formatScore(entry.mean_score)}</Text>
              </Box>
              <Text dim>{entry.description}</Text>
              <Box border="none" class="mkt-card-footer">
                <Text inline dim>by {entry.author}</Text>
                {#if entry.review_count > 0}
                  <Text inline dim>{entry.review_count} review{entry.review_count === 1 ? '' : 's'}</Text>
                {/if}
                {#if isInstalled(entry.id)}
                  <Button variant="outline" size="sm"
                    onclick={() => remove(entry.id)}
                    disabled={removing === entry.id}>
                    {removing === entry.id ? 'Removing…' : 'Remove'}
                  </Button>
                {:else}
                  <Button variant="solid" size="sm"
                    onclick={() => install(entry.id)}
                    disabled={installing === entry.id}>
                    {installing === entry.id ? 'Installing…' : 'Install'}
                  </Button>
                {/if}
              </Box>
            </ListItem>
          {/each}
        </List>
      {/if}
    </Box>
  {/if}

  <!-- Installed panel -->
  {#if activeSection === 'installed'}
    <Box role="tabpanel" class="mkt-panel">
      {#if availableUpdates.length > 0}
        <Box border="none" class="mkt-update-banner" role="status">
          <Text inline>{availableUpdates.length} update{availableUpdates.length === 1 ? '' : 's'} available</Text>
          <Button variant="solid" size="sm"
            onclick={updateAll}
            disabled={updatingAll}>
            {updatingAll ? 'Updating…' : 'Update All'}
          </Button>
        </Box>
      {/if}

      {#if installed.length === 0}
        <Text dim>No procedures installed yet. Use the Discover tab to find and install procedures.</Text>
      {:else}
        <List aria-label="Installed procedures">
          {#each installed as skill (skill.id)}
            {@const hasUpdate = availableUpdates.some(u => u.skill_id === skill.id)}
            <ListItem class="mkt-card {hasUpdate ? 'mkt-has-update' : ''}">
              <Box border="none" class="mkt-card-header">
                <Text inline bold>{skill.name}</Text>
                <Text inline dim>v{skill.version}</Text>
                {#if hasUpdate}
                  <Text inline color="accent" aria-label="Update available">●</Text>
                {/if}
              </Box>
              <Text dim>{skill.description}</Text>
              <Box border="none" class="mkt-card-footer">
                <Text inline dim>
                  Installed {skill.installed_at}
                  {#if skill.last_used}
                    · Last used {skill.last_used}
                  {:else}
                    · Never used
                  {/if}
                </Text>
                <Button variant="outline" size="sm"
                  onclick={() => remove(skill.id)}
                  disabled={removing === skill.id}>
                  {removing === skill.id ? 'Removing…' : 'Remove'}
                </Button>
              </Box>
            </ListItem>
          {/each}
        </List>
      {/if}
    </Box>
  {/if}
</Box>

<style>
  :global(.marketplace-root) {
    display: flex;
    flex-direction: column;
    gap: var(--space-3, 12px);
  }

  :global(.mkt-sections) {
    display: flex;
    gap: 0;
    border-bottom: 1px solid var(--color-border, #3a3a4a);
  }

  :global(.mkt-panel) {
    display: flex;
    flex-direction: column;
    gap: var(--space-3, 12px);
  }

  :global(.mkt-search-row) {
    display: flex;
    gap: 8px;
  }

  :global(.mkt-card) {
    padding: 10px 12px;
    border-radius: var(--radius-sm, 4px);
    border: 1px solid var(--color-border, #3a3a4a);
    background: var(--color-surface, #1e1e2e);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  :global(.mkt-has-update) {
    border-color: var(--color-accent, #7c6af7);
  }

  :global(.mkt-card-header) {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }

  :global(.mkt-card-footer) {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin-top: 4px;
  }

  :global(.mkt-update-banner) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    border-radius: var(--radius-sm, 4px);
    background: color-mix(in srgb, var(--color-accent, #7c6af7) 12%, transparent);
    border: 1px solid var(--color-accent, #7c6af7);
  }
</style>
