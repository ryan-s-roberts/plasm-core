const products = plasm.acme.Product.query({ name: "KitchenSink" })
  .select("id", "name");

const staticLabels = Plan.data(["mirror", "generated"]);

const payloads = Plan.map(products, (product) => ({
  source_id: product.id,
  title: template`Mirror ${product.name}`,
  labels: staticLabels,
}));

Plan.return({ products, staticLabels, payloads });
