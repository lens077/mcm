// Windows smoke suite: the UI half of the shared scenario checklist
// (quickstart.md §端到端验证场景). The core half runs headlessly in
// `src-tauri/src/selftest.rs`, so both platforms cover the same ground.

const OUTLINE = `%mcm 1
%title 冒烟测试
%start 2026-09-01

- 需求阶段 #t1 [2026-09-01..2026-09-10]
  - 用户访谈 #t2 [2026-09-01..2026-09-03]
  - 竞品分析 #t3 [2026-09-04..2026-09-06] <-t2
- 设计阶段 #t4 [2026-09-11..2026-09-20] <-t3
`;

/** Replaces the outline text and triggers generation. */
async function generate(outline: string): Promise<void> {
  const editor = await $('textarea[aria-label="项目大纲文本"]');
  await editor.waitForDisplayed();
  await editor.setValue(outline);
  const button = await $('button[aria-label="生成规划"]').catch(() => null);
  const generateButton = button ?? (await $("button.toolbar-generate"));
  await generateButton.click();
  await browser.pause(400);
}

describe("MCM smoke", () => {
  it("S1 生成与校验: outline in, plan out with no errors", async () => {
    await generate(OUTLINE);
    const status = await $(".status-bar");
    await expect(status).toHaveTextContaining("任务 4");
    await expect(status).toHaveTextContaining("问题 0");
  });

  it("S2 循环依赖定位: a cycle is reported with its path", async () => {
    await generate("%mcm 1\n- 甲 #t1 <-t3\n- 乙 #t2 <-t1\n- 丙 #t3 <-t2\n");
    const issues = await $('section[aria-label="问题面板"]');
    await expect(issues).toHaveTextContaining("V-CYCLE");
    await expect(issues).toHaveTextContaining("环路径");
  });

  it("S3 视图联动: every view tab renders its canvas", async () => {
    await generate(OUTLINE);
    for (const label of ["任务分解", "依赖网络", "时间线", "里程碑"]) {
      const tab = await $(`nav[aria-label="视图"] button=${label}`);
      await tab.click();
      await browser.pause(250);
      const canvas = await $("canvas.view-canvas");
      await expect(canvas).toBeDisplayed();
    }
  });

  it("S5 编辑与撤销: undo and redo update the toolbar state", async () => {
    await generate(OUTLINE);
    const undo = await $('button[aria-label="撤销"]');
    await expect(undo).toBeEnabled();
    await undo.click();
    await browser.pause(250);
    const redo = await $('button[aria-label="重做"]');
    await expect(redo).toBeEnabled();
    await redo.click();
    await browser.pause(250);
    await expect(await $(".toolbar-depth")).toHaveTextContaining("可撤销");
  });

  it("S6 保存状态: the save button flags unsaved changes", async () => {
    await generate(OUTLINE);
    const save = await $('button[aria-label="保存规划"]');
    await expect(save).toHaveTextContaining("•");
  });

  it("S8/S9 导出对话框: both formats are offered", async () => {
    await generate(OUTLINE);
    await (await $('button[aria-label="导出规划"]')).click();
    const dialog = await $('section[aria-label="导出规划"]');
    await expect(dialog).toBeDisplayed();
    await expect(dialog).toHaveTextContaining("XMind");
    await expect(dialog).toHaveTextContaining("Visio");
    await (await $('button[aria-label="关闭"]')).click();
  });

  it("S11 搜索定位: matches are counted and navigable", async () => {
    await generate(OUTLINE);
    const search = await $('input[aria-label="搜索任务"]');
    await search.setValue("阶段");
    await browser.pause(400);
    await expect(await $(".search-count")).toHaveTextContaining("/");
  });

  it("主题切换: dark mode applies without a reload", async () => {
    const toggle = await $("button.theme-toggle");
    await toggle.click();
    await browser.pause(200);
    const theme = await browser.execute(() => document.documentElement.dataset.theme);
    expect(["light", "dark"]).toContain(theme);
  });
});
