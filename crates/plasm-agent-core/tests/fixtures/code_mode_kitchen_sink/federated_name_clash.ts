const acmeProducts = plasm.acme.Product.query({ name: "KitchenSink" })
  .select("id", "name");

const otherProducts = plasm.other.Product.query({ name: "KitchenSink" })
  .select("id", "name");

Plan.return({ acmeProducts, otherProducts });
