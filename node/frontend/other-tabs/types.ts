// 其他 tab 内容类型，对齐后端 src/other/other_tabs。

type OtherDocumentTabItem = {
  key: 'whitepaper';
  title: string;
  contentType: 'document';
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

export type OtherTabItem = OtherDocumentTabItem | OtherRuntimeConstitutionTabItem | OtherTextTabItem;

export type RuntimeConstitutionDocument = {
  html: string;
  blake2_256: string;
  source: string;
};

export type OtherTabsPayload = {
  tabs: OtherTabItem[];
};
