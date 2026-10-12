import { useEffect, useState, useCallback } from 'react'
import { Table, Button, Space, DatePicker, Select, Tag, Typography, Card, Input } from 'antd'
import type { ColumnsType } from 'antd/es/table'
import { useTranslation } from 'react-i18next'
import dayjs from 'dayjs'
import { listLogs } from '@/api/logs'
import { listProviders, type Provider } from '@/api/providers'

const { RangePicker } = DatePicker

interface LogEntry {
  id: number
  created_at: number
  model: string
  provider_id: number
  provider_name: string
  status: number
  duration_ms: number
  first_token_ms: number | null
  input_tokens: number
  output_tokens: number
  total_tokens: number
  estimated_cost: number
  user_id: number | null
  token_name: string | null
  api_key_name: string | null
  error_message: string | null
}

export default function Logs() {
  const { t } = useTranslation()
  const [data, setData] = useState<LogEntry[]>([])
  const [loading, setLoading] = useState(false)
  const [total, setTotal] = useState(0)
  const [page, setPage] = useState(1)
  const [pageSize, setPageSize] = useState(20)
  const [providerId, setProviderId] = useState<number | undefined>(undefined)
  const [model, setModel] = useState<string | undefined>(undefined)
  const [status, setStatus] = useState<string | undefined>(undefined)
  const [tokenName, setTokenName] = useState<string | undefined>(undefined)
  const [dateRange, setDateRange] = useState<[dayjs.Dayjs, dayjs.Dayjs] | null>(null)
  const [providers, setProviders] = useState<Provider[]>([])

  const fetchProviders = async () => {
    try {
      const res = await listProviders(1, 200)
      setProviders(res.data?.data || res.data?.elements || [])
    } catch { /* ignore */ }
  }

  const fetchData = useCallback(async () => {
    setLoading(true)
    try {
      const params: any = { page, size: pageSize }
      if (providerId) params.provider_id = providerId
      if (model) params.model = model
      if (status) params.status = status
      if (tokenName) params.token_name = tokenName
      if (dateRange) {
        params.from = dateRange[0].format('YYYY-MM-DD')
        params.to = dateRange[1].format('YYYY-MM-DD')
      }
      const res = await listLogs(params)
      const result = res.data
      setData(result?.elements ?? [])
      setTotal(result?.total ?? 0)
    } finally {
      setLoading(false)
    }
  }, [page, pageSize, providerId, model, status, tokenName, dateRange])

  useEffect(() => { fetchProviders() }, [])
  useEffect(() => { fetchData() }, [fetchData])

  const handleReset = () => {
    setProviderId(undefined)
    setModel(undefined)
    setStatus(undefined)
    setTokenName(undefined)
    setDateRange(null)
    setPage(1)
  }

  const columns: ColumnsType<LogEntry> = [
    { title: t('common.id'), dataIndex: 'id', key: 'id', width: 60 },
    {
      title: t('dashboard.time'), dataIndex: 'created_at', key: 'created_at', width: 160,
      render: (v) => {
        if (!v) return '-'
        const ts = typeof v === 'number' ? (v > 1e12 ? v : v * 1000) : new Date(v).getTime()
        return dayjs(ts).format('YYYY-MM-DD HH:mm:ss')
      },
    },
    { title: t('dashboard.model'), dataIndex: 'model', key: 'model', ellipsis: true },
    { title: t('dashboard.provider'), dataIndex: 'provider_name', key: 'provider_name' },
    {
      title: t('dashboard.statusCode'), dataIndex: 'status', key: 'status', width: 80,
      render: (v) => v != null ? <Tag color={v < 400 ? 'green' : v < 500 ? 'orange' : 'red'}>{v}</Tag> : '-',
    },
    {
      title: t('dashboard.latency'), dataIndex: 'duration_ms', key: 'duration_ms', width: 100,
      render: (v) => v != null ? `${v}ms` : '-',
    },
    { title: t('logs.inputTokens'), dataIndex: 'input_tokens', key: 'input_tokens', width: 100 },
    { title: t('logs.outputTokens'), dataIndex: 'output_tokens', key: 'output_tokens', width: 100 },
    {
      title: t('logs.keyName'), key: 'key_name',
      render: (_, record) => record.api_key_name || record.token_name || '-',
    },
    {
      title: t('logs.error'), dataIndex: 'error_message', key: 'error_message', ellipsis: true,
      render: (v) => v ? <Typography.Text type="danger" ellipsis={{ tooltip: v }}>{v}</Typography.Text> : '-',
    },
  ]

  return (
    <div style={{ padding: 12 }}>
      <Card size="small" style={{ marginBottom: 16 }}>
        <Space wrap>
          <Select
            allowClear
            placeholder={t('logs.filterProvider')}
            style={{ width: 180 }}
            value={providerId}
            onChange={(v) => { setProviderId(v); setPage(1) }}
          >
            {providers.map((p) => (
              <Select.Option key={p.id} value={p.id}>{p.name}</Select.Option>
            ))}
          </Select>
          <Input
            allowClear
            placeholder={t('logs.filterModel')}
            style={{ width: 200 }}
            value={model}
            onChange={(e) => { setModel(e.target.value || undefined); setPage(1) }}
          />
          <Input
            allowClear
            placeholder={t('logs.filterKeyName')}
            style={{ width: 180 }}
            value={tokenName}
            onChange={(e) => { setTokenName(e.target.value || undefined); setPage(1) }}
          />
          <Select
            allowClear
            placeholder={t('logs.filterStatus')}
            style={{ width: 140 }}
            value={status}
            onChange={(v) => { setStatus(v); setPage(1) }}
          >
            <Select.Option value="success">{t('logs.statusSuccess')}</Select.Option>
            <Select.Option value="client_error">{t('logs.statusClientError')}</Select.Option>
            <Select.Option value="server_error">{t('logs.statusServerError')}</Select.Option>
          </Select>
          <RangePicker
            value={dateRange}
            onChange={(dates) => { setDateRange(dates as any); setPage(1) }}
          />
          <Button onClick={handleReset}>
            {t('logs.reset')}
          </Button>
        </Space>
      </Card>

      <Table
        columns={columns}
        dataSource={data}
        rowKey="id"
        loading={loading}
        pagination={{
          current: page,
          pageSize,
          total,
          showSizeChanger: true,
          showQuickJumper: total > 50,
          showTotal: (t) => `共 ${t} 条`,
          onChange: (p, ps) => { setPage(p); setPageSize(ps) },
        }}
        size="small"
        scroll={{ x: 1100 }}
      />
    </div>
  )
}
