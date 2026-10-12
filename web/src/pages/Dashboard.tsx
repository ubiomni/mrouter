import { useEffect, useState } from 'react';
import { Card, Col, Row, Statistic, Table, Tag, Typography, Spin, Space } from 'antd';
import { ApiOutlined, CloudServerOutlined, KeyOutlined, ThunderboltOutlined, DollarOutlined } from '@ant-design/icons';
import { useTranslation } from 'react-i18next';
import ReactECharts from 'echarts-for-react';
import dayjs from 'dayjs';
import { getOverviewStats, getProxyStatus, getRecentLogs } from '../api/dashboard';

interface OverviewStats {
  total_requests_today: number;
  active_providers: number;
  active_api_keys: number;
  total_tokens_today: number;
  total_input_tokens_today: number;
  total_output_tokens_today: number;
  total_cache_creation_tokens_today: number;
  total_cache_read_tokens_today: number;
  total_cost_today: number;
  avg_cost_per_request: number;
  requests_per_hour: { hour: string; count: number }[];
  requests_by_provider_type: { provider_type: string; count: number }[];
  top_models: { model: string; count: number }[];
  by_provider: { provider_name: string; requests: number; input_tokens: number; output_tokens: number; cache_creation_tokens: number; cache_read_tokens: number; tokens: number; cost: number }[];
}

interface ProxyStatus {
  status: string;
  request_count: number;
  current_provider: string | null;
  failover_queue_size: number;
}

interface LogEntry {
  id?: number;
  request_time: number;
  model: string;
  provider_name: string;
  status: number;
  duration_ms: number;
}

export default function Dashboard() {
  const { t } = useTranslation();
  const [loading, setLoading] = useState(true);
  const [stats, setStats] = useState<OverviewStats | null>(null);
  const [proxyStatus, setProxyStatus] = useState<ProxyStatus | null>(null);
  const [recentLogs, setRecentLogs] = useState<LogEntry[]>([]);

  useEffect(() => {
    const fetchData = async () => {
      setLoading(true);
      try {
        const [statsRes, statusRes, logsRes] = await Promise.allSettled([
          getOverviewStats(),
          getProxyStatus(),
          getRecentLogs(10),
        ]);
        if (statsRes.status === 'fulfilled') setStats(statsRes.value.data?.data ?? statsRes.value.data);
        if (statusRes.status === 'fulfilled') setProxyStatus(statusRes.value.data?.data ?? statusRes.value.data);
        if (logsRes.status === 'fulfilled') {
          const logsData = logsRes.value.data?.data;
          const elements = logsData?.elements || [];
          setRecentLogs(Array.isArray(elements) ? elements : []);
        }
      } finally {
        setLoading(false);
      }
    };
    fetchData();
  }, []);

  if (loading) return <Spin size="large" style={{ display: 'block', margin: '100px auto' }} />;

  const requestsPerHourOption = {
    xAxis: { type: 'category' as const, data: stats?.requests_per_hour?.map((h) => `${h.hour}:00`) || [] },
    yAxis: { type: 'value' as const },
    series: [{ data: stats?.requests_per_hour?.map((h) => h.count) || [], type: 'bar' as const, color: '#1677ff' }],
    tooltip: { trigger: 'axis' as const },
    grid: { left: 40, right: 20, top: 20, bottom: 30 },
  };

  const byTypeOption = {
    tooltip: { trigger: 'item' as const },
    series: [{
      type: 'pie' as const,
      radius: ['40%', '70%'],
      data: stats?.requests_by_provider_type?.map((r) => ({ value: r.count, name: r.provider_type })) || [],
    }],
  };

  const logColumns = [
    {
      title: t('dashboard.time'), dataIndex: 'request_time', key: 'request_time',
      render: (v: number) => {
        if (!v) return '-';
        const ts = typeof v === 'number' ? (v > 1e12 ? v : v * 1000) : new Date(v).getTime();
        return dayjs(ts).format('HH:mm:ss');
      },
    },
    { title: t('dashboard.model'), dataIndex: 'model', key: 'model' },
    { title: t('dashboard.provider'), dataIndex: 'provider_name', key: 'provider_name' },
    { title: t('dashboard.statusCode'), dataIndex: 'status', key: 'status', render: (v: number) => v ? <Tag color={v < 400 ? 'green' : 'red'}>{v}</Tag> : '-' },
    { title: t('dashboard.latency'), dataIndex: 'duration_ms', key: 'duration_ms', render: (v: number) => v != null ? `${v}ms` : '-' },
  ];

  const providerColumns = [
    { title: t('dashboard.provider'), dataIndex: 'provider_name', key: 'provider_name' },
    { title: t('dashboard.totalRequests'), dataIndex: 'requests', key: 'requests' },
    { title: t('dashboard.inputTokens'), dataIndex: 'input_tokens', key: 'input_tokens', render: (v: number) => v?.toLocaleString() },
    { title: t('dashboard.outputTokens'), dataIndex: 'output_tokens', key: 'output_tokens', render: (v: number) => v?.toLocaleString() },
    { title: t('dashboard.cacheCreation'), dataIndex: 'cache_creation_tokens', key: 'cache_creation_tokens', render: (v: number) => v?.toLocaleString() },
    { title: t('dashboard.cacheRead'), dataIndex: 'cache_read_tokens', key: 'cache_read_tokens', render: (v: number) => v?.toLocaleString() },
    { title: t('dashboard.totalTokens'), dataIndex: 'tokens', key: 'tokens', render: (v: number) => v?.toLocaleString() },
    { title: t('dashboard.cost'), dataIndex: 'cost', key: 'cost', render: (v: number) => `$${v?.toFixed(4)}` },
  ];

  return (
    <div>
      <Row gutter={[16, 16]}>
        <Col xs={24} sm={12} lg={4}>
          <Card><Statistic title={t('dashboard.totalRequests')} value={stats?.total_requests_today ?? 0} prefix={<ApiOutlined />} /></Card>
        </Col>
        <Col xs={24} sm={12} lg={4}>
          <Card><Statistic title={t('dashboard.inputTokens')} value={stats?.total_input_tokens_today ?? 0} /></Card>
        </Col>
        <Col xs={24} sm={12} lg={4}>
          <Card><Statistic title={t('dashboard.outputTokens')} value={stats?.total_output_tokens_today ?? 0} /></Card>
        </Col>
        <Col xs={24} sm={12} lg={4}>
          <Card><Statistic title={t('dashboard.cacheCreation')} value={stats?.total_cache_creation_tokens_today ?? 0} /></Card>
        </Col>
        <Col xs={24} sm={12} lg={4}>
          <Card><Statistic title={t('dashboard.cacheRead')} value={stats?.total_cache_read_tokens_today ?? 0} /></Card>
        </Col>
        <Col xs={24} sm={12} lg={4}>
          <Card><Statistic title={t('dashboard.totalCost')} value={stats?.total_cost_today ?? 0} precision={4} prefix={<DollarOutlined />} suffix="USD" /></Card>
        </Col>
      </Row>
      <Row gutter={[16, 16]} style={{ marginTop: 16 }}>
        <Col xs={24} sm={12} lg={8}>
          <Card><Statistic title={t('dashboard.totalTokens')} value={stats?.total_tokens_today ?? 0} prefix={<ThunderboltOutlined />} /></Card>
        </Col>
        <Col xs={24} sm={12} lg={8}>
          <Card><Statistic title={t('dashboard.activeProviders')} value={stats?.active_providers ?? 0} prefix={<CloudServerOutlined />} /></Card>
        </Col>
        <Col xs={24} sm={12} lg={8}>
          <Card><Statistic title={t('dashboard.activeKeys')} value={stats?.active_api_keys ?? 0} prefix={<KeyOutlined />} /></Card>
        </Col>
      </Row>

      <Row gutter={[16, 16]} style={{ marginTop: 16 }}>
        <Col xs={24} lg={16}>
          <Card title={t('dashboard.requestsPerHour')} size="small">
            <ReactECharts option={requestsPerHourOption} style={{ height: 250 }} />
          </Card>
        </Col>
        <Col xs={24} lg={8}>
          <Card title={t('dashboard.requestsByType')} size="small">
            <ReactECharts option={byTypeOption} style={{ height: 250 }} />
          </Card>
        </Col>
      </Row>

      <Row gutter={[16, 16]} style={{ marginTop: 16 }}>
        <Col xs={24} lg={12}>
          <Card title={t('dashboard.topModels')} size="small">
            {stats?.top_models?.length ? stats.top_models.map((m) => (
              <div key={m.model} style={{ display: 'flex', justifyContent: 'space-between', padding: '4px 0' }}>
                <Typography.Text>{m.model}</Typography.Text>
                <Tag>{m.count}</Tag>
              </div>
            )) : <Typography.Text type="secondary">-</Typography.Text>}
          </Card>
        </Col>
        <Col xs={24} lg={12}>
          <Card title={t('dashboard.proxyStatus')} size="small">
            <Space direction="vertical" style={{ width: '100%' }}>
              <Tag color={proxyStatus?.status === 'running' ? 'green' : 'red'}>
                {proxyStatus?.status === 'running' ? t('dashboard.running') : t('dashboard.stopped')}
              </Tag>
              {proxyStatus && (
                <div>
                  <Typography.Text type="secondary" style={{ fontSize: 12 }}>
                    {t('dashboard.totalRequests')}: {proxyStatus.request_count?.toLocaleString()}
                  </Typography.Text>
                  {proxyStatus.current_provider && (
                    <div><Typography.Text type="secondary" style={{ fontSize: 12 }}>
                      Current: {proxyStatus.current_provider}
                    </Typography.Text></div>
                  )}
                </div>
              )}
            </Space>
          </Card>
        </Col>
      </Row>

      <Row gutter={[16, 16]} style={{ marginTop: 16 }}>
        <Col span={24}>
          <Card title={t('dashboard.costByProvider')} size="small">
            <Table
              columns={providerColumns}
              dataSource={stats?.by_provider || []}
              rowKey="provider_name"
              pagination={false}
              size="small"
              scroll={{ x: 600 }}
            />
          </Card>
        </Col>
      </Row>

      <Row gutter={[16, 16]} style={{ marginTop: 16 }}>
        <Col span={24}>
          <Card title={t('dashboard.recentActivity')} size="small">
            <Table
              columns={logColumns}
              dataSource={recentLogs}
              rowKey={(r) => r.id?.toString() ?? `${r.request_time}-${r.model}`}
              pagination={false}
              size="small"
              scroll={{ x: 600 }}
            />
          </Card>
        </Col>
      </Row>
    </div>
  );
}
