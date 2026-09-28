/** Profiles：环境档案的可视化组合编辑器（M1）。 */

import React from "react";
import { PlannedPage } from "../components/ui";

export function ProfilesPage() {
  return (
    <PlannedPage
      icon="profiles"
      title="Profiles · 环境档案"
      milestone="M1"
      summary="把五类资源组合成一套「环境档案」，再把档案绑定到 Agent 目标。同步时由声明式 Adapter 生成各 Agent 需要的配置格式。"
      bullets={[
        {
          title: "可视化组合编辑器",
          detail: "左侧资源库、右侧档案内容区，拖拽或点击即可把资源加入档案，每项可做 per-profile 覆盖。",
        },
        {
          title: "档案对比视图",
          detail: "多个档案并排对比，一眼看清「日常开发」与「轻量问答」差在哪几个资源。",
        },
        {
          title: "一键应用到 Agent",
          detail: "从档案卡片直接进入同步流程，先看 diff 再确认写入，自动备份并可回滚。",
        },
        {
          title: "导入 / 导出整套环境",
          detail: "一个 JSON 文件带走完整环境定义（密钥默认剔除），用于换机迁移或团队共享。",
        },
      ]}
    />
  );
}