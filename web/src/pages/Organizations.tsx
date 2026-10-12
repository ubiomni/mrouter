import { useEffect, useState } from 'react'
import { Table, Button, Space, Modal, Form, Input, Select, Tag, App } from 'antd'
import type { ColumnsType } from 'antd/es/table'
import { useTranslation } from 'react-i18next'
import dayjs from 'dayjs'
import { listOrganizations, createOrganization, updateOrganization } from '@/api/organizations'

interface Organization {
  id: number
  name: string
  display_name: string | null
  is_active: boolean
  created_at: number
  updated_at: number
}

export default function Organizations() {
  const { message } = App.useApp()
  const { t } = useTranslation()
  const [data, setData] = useState<Organization[]>([])
  const [loading, setLoading] = useState(false)
  const [modalOpen, setModalOpen] = useState(false)
  const [editingOrg, setEditingOrg] = useState<Organization | null>(null)
  const [form] = Form.useForm()

  const fetchData = async () => {
    setLoading(true)
    try {
      const res = await listOrganizations()
      setData(res.data ?? [])
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => { fetchData() }, [])

  const handleCreate = () => {
    setEditingOrg(null)
    form.resetFields()
    setModalOpen(true)
  }

  const handleEdit = (record: Organization) => {
    setEditingOrg(record)
    form.setFieldsValue({
      name: record.name,
      display_name: record.display_name,
      is_active: record.is_active,
    })
    setModalOpen(true)
  }

  const handleSubmit = async () => {
    try {
      const values = await form.validateFields()
      if (editingOrg) {
        await updateOrganization(editingOrg.id, values)
        message.success(t('orgs.updated'))
      } else {
        await createOrganization(values)
        message.success(t('orgs.created'))
      }
      setModalOpen(false)
      fetchData()
    } catch {
      message.error(t('orgs.formError'))
    }
  }

  const columns: ColumnsType<Organization> = [
    { title: t('common.id'), dataIndex: 'id', key: 'id', width: 60 },
    { title: t('orgs.name'), dataIndex: 'name', key: 'name' },
    { title: t('orgs.displayName'), dataIndex: 'display_name', key: 'display_name', render: (v) => v || '-' },
    {
      title: t('common.status'), dataIndex: 'is_active', key: 'is_active',
      render: (v) => <Tag color={v ? 'green' : 'default'}>{v ? t('common.active') : t('common.inactive')}</Tag>,
    },
    {
      title: t('common.createdAt'), dataIndex: 'created_at', key: 'created_at',
      render: (v) => v ? dayjs.unix(v).format('YYYY-MM-DD HH:mm') : '-',
    },
    {
      title: t('common.actions'), key: 'actions', width: 100,
      render: (_, record) => (
        <Button type="link" size="small" onClick={() => handleEdit(record)}>{t('common.edit')}</Button>
      ),
    },
  ]

  return (
    <div style={{ padding: 12 }}>
      <Space style={{ marginBottom: 16 }}>
        <Button type="primary" onClick={handleCreate}>{t('orgs.add')}</Button>
      </Space>

      <Table columns={columns} dataSource={data} rowKey="id" loading={loading} pagination={false} />

      <Modal
        title={editingOrg ? t('orgs.edit') : t('orgs.create')}
        open={modalOpen}
        onOk={handleSubmit}
        onCancel={() => setModalOpen(false)}
        destroyOnHidden
      >
        <Form form={form} layout="vertical">
          <Form.Item name="name" label={t('orgs.name')} rules={[{ required: true, message: t('orgs.nameRequired') }]}>
            <Input />
          </Form.Item>
          <Form.Item name="display_name" label={t('orgs.displayName')}>
            <Input />
          </Form.Item>
          {editingOrg && (
            <Form.Item name="is_active" label={t('orgs.active')}>
              <Select>
                <Select.Option value={true}>{t('common.active')}</Select.Option>
                <Select.Option value={false}>{t('common.inactive')}</Select.Option>
              </Select>
            </Form.Item>
          )}
        </Form>
      </Modal>
    </div>
  )
}
