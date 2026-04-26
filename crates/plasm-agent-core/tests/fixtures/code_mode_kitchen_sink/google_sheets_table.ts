const products = plasm.acme.Product.query({ name: "KitchenSink" })
  .where(field("id").contains("p"))
  .select("id", "name");

const valueRange = Plan.data({
  values: [
    ["Owner", "Points"],
    ["Ada", 5],
    ["Grace", 8],
  ],
});

const rows = Plan.table(valueRange, {
  columns: ["owner", "points"],
  hasHeader: true,
});

const totals = Plan.aggregate(rows, [
  { name: "total_points", function: "sum", field: ["points"] },
  { name: "row_count", function: "count" },
]);

const productRows = Plan.project(products, {
  product_id: (product) => product.id,
  product_name: (product) => product.name,
});

Plan.return([products, rows, totals, productRows]);
