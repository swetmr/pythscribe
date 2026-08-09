import { defineBehaviorSuite, expect, user, elementsWithPercentWidth, buttonsOrTabs } from './_harness'

// Contract: CourseCatalog() — tabs All/Enrolled/Completed filter >=6 courses;
// each card shows a progress bar whose width style = progress percent.
defineBehaviorSuite('macro_course_cards', 'CourseCatalog', async ({ mount }) => {
  const u = user()
  const { container } = mount()

  // the three filter tabs exist (labels may carry a count, e.g. "All (7)");
  // tolerance: filter controls are legitimately role=button OR role=tab
  const allTab = buttonsOrTabs(/^\s*All\b/i)[0]
  expect(allTab).toBeTruthy()
  expect(buttonsOrTabs(/^\s*Enrolled\b/i).length).toBeGreaterThan(0)
  const completedTab = buttonsOrTabs(/^\s*Completed\b/i)[0]
  expect(completedTab).toBeTruthy()

  // at least one progress bar carries an inline width:%% style
  expect(elementsWithPercentWidth(container).length).toBeGreaterThan(0)

  // switching to 'Completed' changes the rendered set (filter is live)
  const before = container.innerHTML
  await u.click(completedTab)
  expect(container.innerHTML).not.toBe(before)
})
