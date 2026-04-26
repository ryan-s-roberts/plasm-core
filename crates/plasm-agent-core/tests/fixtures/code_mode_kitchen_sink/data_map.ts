const products = plasm.acme.Product.query({ name: "KitchenSink" })
  .select("id", "name");

const staticLabels = Plan.data({ label: "mirror" });

const payloads = Plan.map(products, (product) => ({
  source_id: product.id,
  title: template`Mirror ${product.name}`,
  label: staticLabels.label,
}));

Plan.return([products, staticLabels, payloads]);
