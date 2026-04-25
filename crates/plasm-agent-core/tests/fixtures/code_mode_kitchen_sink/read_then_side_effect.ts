const stale = plasm.acme.Product.query({ name: "KitchenSink" })
  .where(field("id").eq(daysAgo(30)))
  .select("id", "name");

const labelStale = forEach(stale, (product) =>
  plasm.acme.Product.ref(product.id).action("add-label", {
    label: "stale",
    title: product.name,
  }),
);

Plan.return({ stale, labelStale });
