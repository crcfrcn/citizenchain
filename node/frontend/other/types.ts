// 其他 tab 内容类型，对齐后端 src/other/other_tabs。

export type LocalDoc = {
  key: 'whitepaper';
  markdown: string;
};

export type WhitepaperDocument = {
  markdown: string;
};

type OtherRuntimeConstitutionTabItem = {
  key: 'constitution';
  title: string;
  contentType: 'runtimeConstitution';
};

type OtherTextTabItem = {
  key: string;
  title: string;
  contentType: 'text';
  text: string;
};

export type OtherTabItem = OtherRuntimeConstitutionTabItem | OtherTextTabItem;

export type RuntimeConstitutionDocument = {
  html: string;
  blake2_256: string;
  source: string;
};

export type OtherTabsPayload = {
  tabs: OtherTabItem[];
};
