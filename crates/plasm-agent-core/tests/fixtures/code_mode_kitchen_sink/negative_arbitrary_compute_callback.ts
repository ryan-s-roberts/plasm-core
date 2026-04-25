const issues = Plan.data([{ id: "i1", state: { name: "Todo" } }]);

const invalid = Plan.project(issues, {
  label: () => "not symbolic",
});

Plan.return({ invalid });
