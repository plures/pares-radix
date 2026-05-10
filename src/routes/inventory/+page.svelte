<script lang="ts">
import { Box, Card, Heading, Text } from '@plures/design-dojo';
</script>

<svelte:head>
	<title>Inventory — Radix</title>
</svelte:head>

<Heading level={1}>Inventory</Heading>

<Box class="inventory-layout">
<Card class="inventory-card">
<Heading level={2}>Inventory module is being migrated</Heading>
<Text as="p">
This route is temporarily in maintenance while inventory workflows are migrated to
PluresDB-backed procedures and design-dojo-only rendering.
</Text>
<Text as="p">
The UI will return once the migration is complete and all persistence flows through the
reactive runtime.
</Text>
</Card>
</Box>

<style>
:global(.inventory-layout) {
margin-top: 1rem;
}

:global(.inventory-card) {
max-width: 720px;
}
</style>
