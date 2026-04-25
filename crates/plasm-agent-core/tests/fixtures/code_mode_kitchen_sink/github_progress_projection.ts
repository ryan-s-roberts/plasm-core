const issues = plasm.acme.Product.query({ name: "KitchenSink" })
  .where(field("name").contains("Kitchen"))
  .select("id", "name");

const progress = Plan.project(issues, {
  number: (issue) => issue.id,
  title: (issue) => issue.name,
});

const labelPayloads = Plan.map(progress, (issue) => ({
  title: template`Progress ${issue.title}`,
}));

const labelProgress = forEach(labelPayloads, (payload) =>
  plasm.acme.Product.ref(payload.title).action("add-label", {
    label: "computed-progress",
    title: payload.title,
  }),
);

Plan.return({ issues, progress, labelPayloads, labelProgress });
