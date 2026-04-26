const sourceProducts = plasm.acme.Product.query({ name: "KitchenSink" })
  .select("id", "name");

const createDerived = forEach(sourceProducts, (product) =>
  plasm.acme.Product.create({
    source_id: product.id,
    title: template`Mirror ${product.name}`,
  }),
);

Plan.return([sourceProducts, createDerived]);
