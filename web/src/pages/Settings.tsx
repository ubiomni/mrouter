import { useEffect, useState } from 'react'
import {
  Table, Button, Space, Modal, Form, Input, Tabs, Tag, Popconfirm, App, Card, Upload, Descriptions, Typography,
} from 'antd'
import { UploadOutlined, DownloadOutlined } from '@ant-design/icons'
import type { ColumnsType } from 'antd/es/table'
import { useTranslation } from 'react-i18next'
import { listSettings, upsertSetting, deleteSetting } from '@/api/settings'
import { exportMigration, importMigration } from '@/api/migration'

interface Setting {
  key: string
  value: any
  description: string | null
  category: string | null
}

const CATEGORY_GROUPS: Record<string, string[]> = {
  Pricing: ['ModelRatio', 'CompletionRatio', 'GroupRatio'],
  ParamOverride: ['ParamOverride'],
}

function categorize(settings: Setting[]): Record<string, Setting[]> {
  const result: Record<string, Setting[]> = { Pricing: [], ParamOverride: [], Other: [] }
  for (const s of settings) {
    let placed = false
    for (const [group, keys] of Object.entries(CATEGORY_GROUPS)) {
      if (keys.some((k) => s.key.startsWith(k)) || s.category === group.toLowerCase()) {
        result[group].push(s)
        placed = true
        break
      }
    }
    if (!placed) result['Other'].push(s)
  }
  return result
}

export default function Settings() {
  const { message } = App.useApp()
  const { t } = useTranslation()
  const [data, setData] = useState<Setting[]>([])
  const [loading, setLoading] = useState(false)
  const [modalOpen, setModalOpen] = useState(false)
  const [editingSetting, setEditingSetting] = useState<Setting | null>(null)
  const [form] = Form.useForm()
  const [importResult, setImportResult] = useState<any>(null)
  const [migrationLoading, setMigrationLoading] = useState(false)

  const fetchData = async () => {
    setLoading(true)
    try {
      const res = await listSettings()
      setData(res.data ?? [])
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => { fetchData() }, [])

  const handleCreate = () => {
    setEditingSetting(null)
    form.resetFields()
    setModalOpen(true)
  }

  const handleEdit = (record: Setting) => {
    setEditingSetting(record)
    const valueStr = typeof record.value === 'string' ? record.value : JSON.stringify(record.value, null, 2)
    form.setFieldsValue({ key: record.key, value: valueStr, description: record.description })
    setModalOpen(true)
  }

  const handleDelete = async (key: string) => {
    try {
      await deleteSetting(key)
      message.success(t('settings.deleted'))
      fetchData()
    } catch {
      message.error(t('settings.deleteFailed'))
    }
  }

  const handleSubmit = async () => {
    try {
      const values = await form.validateFields()
      let parsedValue: any = values.value
      try { parsedValue = JSON.parse(values.value) } catch { /* keep as string */ }
      await upsertSetting(values.key, { value: parsedValue, description: values.description })
      message.success(t('settings.saved'))
      setModalOpen(false)
      fetchData()
    } catch {
      message.error(t('settings.formError'))
    }
  }

  const renderValuePreview = (value: any) => {
    if (value === null || value === undefined) return '-'
    const str = typeof value === 'string' ? value : JSON.stringify(value)
    if (str.length > 80) return str.slice(0, 80) + '...'
    return str
  }

  const columns: ColumnsType<Setting> = [
    { title: t('settings.key'), dataIndex: 'key', key: 'key' },
    { title: t('settings.description'), dataIndex: 'description', key: 'description', render: (v) => v || '-' },
    { title: t('settings.value'), dataIndex: 'value', key: 'value', ellipsis: true, render: (v) => <code>{renderValuePreview(v)}</code> },
    {
      title: t('common.actions'), key: 'actions', width: 150,
      render: (_, record) => (
        <Space size="small">
          <Button type="link" size="small" onClick={() => handleEdit(record)}>{t('common.edit')}</Button>
          <Popconfirm title={t('settings.deleteConfirm')} onConfirm={() => handleDelete(record.key)} okText={t('common.yes')} cancelText={t('common.no')}>
            <Button type="link" size="small" danger>{t('common.delete')}</Button>
          </Popconfirm>
        </Space>
      ),
    },
  ]

  const grouped = categorize(data)
  return (
    <div style={{ padding: 12 }}>
      <Space style={{ marginBottom: 16 }}>
        <Button type="primary" onClick={handleCreate}>{t('settings.add')}</Button>
      </Space>

      <Tabs
        defaultActiveKey="Pricing"
        items={Object.entries(grouped).map(([group, items]) => ({
          key: group,
          label: <span>{group} <Tag>{items.length}</Tag></span>,
          children: (
            <Table columns={columns} dataSource={items} rowKey="key" loading={loading} pagination={false} size="middle" />
          ),
        }))}
      />

      <Modal
        title={editingSetting ? t('settings.edit') : t('settings.add')}
        open={modalOpen}
        onOk={handleSubmit}
        onCancel={() => setModalOpen(false)}
        destroyOnHidden
        width={600}
      >
        <Form form={form} layout="vertical">
          <Form.Item name="key" label={t('settings.key')} rules={[{ required: true, message: t('settings.keyRequired') }]}>
            <Input disabled={!!editingSetting} placeholder={t('settings.keyPlaceholder')} />
          </Form.Item>
          <Form.Item name="description" label={t('settings.description')}>
            <Input />
          </Form.Item>
          <Form.Item name="value" label={t('settings.valueLabel')} rules={[{ required: true, message: t('settings.valueRequired') }]}>
            <Input.TextArea rows={8} style={{ fontFamily: 'monospace' }} placeholder='{"model-name": 1.0}' />
          </Form.Item>
        </Form>
      </Modal>

      <Card title={t('settings.migration', 'Data Migration')} style={{ marginTop: 24 }}>
        <Typography.Paragraph type="secondary">
          {t('settings.migrationDesc', 'Export or import organizations, users, and API keys for instance migration.')}
        </Typography.Paragraph>
        <Space>
          <Button
            icon={<DownloadOutlined />}
            loading={migrationLoading}
            onClick={async () => {
              setMigrationLoading(true)
              try {
                const res = await exportMigration()
                const blob = new Blob([JSON.stringify(res.data, null, 2)], { type: 'application/json' })
                const url = URL.createObjectURL(blob)
                const a = document.createElement('a')
                a.href = url
                a.download = 'mrouter-migration.json'
                a.click()
                URL.revokeObjectURL(url)
                message.success(t('settings.exportSuccess', 'Export completed'))
              } catch {
                message.error(t('settings.exportFailed', 'Export failed'))
              } finally {
                setMigrationLoading(false)
              }
            }}
          >
            {t('settings.exportMigration', 'Export')}
          </Button>
          <Upload
            accept=".json"
            showUploadList={false}
            beforeUpload={(file) => {
              const reader = new FileReader()
              reader.onload = async (e) => {
                setMigrationLoading(true)
                setImportResult(null)
                try {
                  const data = JSON.parse(e.target?.result as string)
                  const res = await importMigration(data)
                  setImportResult(res.data)
                  message.success(t('settings.importSuccess', 'Import completed'))
                } catch {
                  message.error(t('settings.importFailed', 'Import failed'))
                } finally {
                  setMigrationLoading(false)
                }
              }
              reader.readAsText(file)
              return false
            }}
          >
            <Button icon={<UploadOutlined />} loading={migrationLoading}>
              {t('settings.importMigration', 'Import')}
            </Button>
          </Upload>
        </Space>
        {importResult && (
          <Descriptions column={4} size="small" style={{ marginTop: 16 }} bordered>
            <Descriptions.Item label="Orgs">
              <Tag color="green">{importResult.orgs?.created ?? 0} created</Tag>
              <Tag color="blue">{importResult.orgs?.updated ?? 0} updated</Tag>
            </Descriptions.Item>
            <Descriptions.Item label="Users">
              <Tag color="green">{importResult.users?.created ?? 0} created</Tag>
              <Tag color="blue">{importResult.users?.updated ?? 0} updated</Tag>
            </Descriptions.Item>
            <Descriptions.Item label="API Keys">
              <Tag color="green">{importResult.keys?.created ?? 0} created</Tag>
              <Tag>{importResult.keys?.skipped ?? 0} skipped</Tag>
            </Descriptions.Item>
          </Descriptions>
        )}
      </Card>
    </div>
  )
}
