// 机构资料库 — 自治模块,管理机构注册文件(公司章程/许可证等)。
// 本组件独立管理数据加载、上传、下载、删除,外部只需传 auth + cidNumber + canWrite。
// 修改资料库功能只需改本文件,不影响其他模块。

import React, { useCallback, useEffect, useState } from 'react';
import { Button, Card, Popconfirm, Select, Table, Tag, Upload } from 'antd';
import { UploadOutlined, DownloadOutlined, DeleteOutlined } from '@ant-design/icons';
import type { UploadFile } from 'antd/es/upload/interface';
import {
  deleteDocument,
  DOC_TYPE_OPTIONS,
  downloadDocument,
  type InstitutionDocument,
  listDocuments,
  uploadDocument,
} from './api';
import type { AdminAuth } from '../auth/types';
import { notice } from '../utils/notice';

interface Props {
  auth: AdminAuth;
  cidNumber: string;
  canWrite: boolean;
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

const DOC_TYPE_COLORS: Record<string, string> = {
  '公司章程': 'blue',
  '营业许可证': 'green',
  '股东会决议': 'orange',
  '法人授权书': 'purple',
  '其他': 'default',
};

export const DocsLibrary: React.FC<Props> = ({
  auth,
  cidNumber,
  canWrite,
}) => {
  const [docs, setDocs] = useState<InstitutionDocument[]>([]);
  const [loading, setLoading] = useState(false);
  const [uploading, setUploading] = useState(false);
  const [selectedDocType, setSelectedDocType] = useState<string>('其他');

  const load = useCallback(() => {
    setLoading(true);
    listDocuments(auth, cidNumber)
      .then(setDocs)
      .catch((err) => notice.error(err, ''))
      .finally(() => setLoading(false));
  }, [auth.access_token, cidNumber]);

  useEffect(() => {
    load();
  }, [load]);

  const onUpload = async (file: UploadFile) => {
    const rawFile = file as unknown as File;
    if (!rawFile || !rawFile.name) return false;
    setUploading(true);
    try {
      // 上传资料属本地写(Passkey 档):uploadDocument 内部只带 passkey 断言提交,不走冷签。
      await uploadDocument(auth, cidNumber, rawFile, selectedDocType);
      notice.success('文件上传成功');
      load();
    } catch (err) {
      notice.error(err, '');
    } finally {
      setUploading(false);
    }
    return false; // 阻止 antd 默认上传
  };

  const onDownload = async (doc: InstitutionDocument) => {
    try {
      await downloadDocument(auth, cidNumber, doc.id, doc.file_name);
    } catch (err) {
      notice.error(err, '');
    }
  };

  const onDelete = async (doc: InstitutionDocument) => {
    try {
      // 删除资料属本地写(Passkey 档):deleteDocument 内部只带 passkey 断言提交,不走冷签。
      await deleteDocument(auth, cidNumber, doc.id);
      notice.success('文件已删除');
      load();
    } catch (err) {
      notice.error(err, '');
    }
  };

  return (
    <Card
      type="inner"
      title={`资料库(${docs.length})`}
      extra={
        canWrite && (
          <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
            <Select
              value={selectedDocType}
              onChange={setSelectedDocType}
              style={{ width: 130 }}
              options={DOC_TYPE_OPTIONS.map((t) => ({ value: t, label: t }))}
            />
            <Upload
              beforeUpload={onUpload}
              showUploadList={false}
              accept=".pdf,.doc,.docx,.jpg,.jpeg,.png,.xls,.xlsx"
            >
              <Button icon={<UploadOutlined />} loading={uploading} type="primary">
                上传文件
              </Button>
            </Upload>
          </div>
        )
      }
    >
      <Table<InstitutionDocument>
        rowKey="id"
        loading={loading}
        dataSource={docs}
        pagination={docs.length > 10 ? { pageSize: 10 } : false}
        columns={[
          { title: '文件名', dataIndex: 'file_name', ellipsis: true },
          {
            title: '类型',
            dataIndex: 'doc_type',
            width: 120,
            render: (v: string) => <Tag color={DOC_TYPE_COLORS[v] || 'default'}>{v}</Tag>,
          },
          {
            title: '大小',
            dataIndex: 'file_size',
            width: 100,
            render: (v: number) => formatFileSize(v),
          },
          {
            title: '上传时间',
            dataIndex: 'uploaded_at',
            width: 170,
            render: (v: string) => new Date(v).toLocaleString('zh-CN'),
          },
          {
            title: '操作',
            width: 120,
            align: 'center',
            render: (_v, row) => (
              <div style={{ display: 'flex', gap: 4, justifyContent: 'center' }}>
                <Button
                  size="small"
                  type="link"
                  icon={<DownloadOutlined />}
                  onClick={() => onDownload(row)}
                />
                {canWrite && (
                  <Popconfirm
                    title={`确认删除 "${row.file_name}"?`}
                    onConfirm={() => onDelete(row)}
                    okText="删除"
                    okButtonProps={{ danger: true }}
                    cancelText="取消"
                  >
                    <Button size="small" danger type="link" icon={<DeleteOutlined />} />
                  </Popconfirm>
                )}
              </div>
            ),
          },
        ]}
      />
    </Card>
  );
};
