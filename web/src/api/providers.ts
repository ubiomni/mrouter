import client from './client'

export enum ProviderStatus {
  Enabled = 1,
  ManuallyDisabled = 2,
  AutoDisabled = 3,
}

export interface Provider {
  id: number
  org_id: number
  name: string
  display_name: string | null
  preset_key: string | null
  base_url: string
  api_key: string
  priority: number
  weight: number
  provider_group: string
  category: string
  rpm_limit: number
  tpm_limit: number
  concurrent_limit: number
  auth_method: string
  custom_headers: string | Record<string, string> | null
  status: ProviderStatus
  auto_ban: boolean
  remark: string | null
  created_at: string
  updated_at: string
}

export interface PresetEndpoint {
  endpoint_type: string
  path: string
  format: string
}

export interface ProviderPreset {
  preset_key: string
  display_name: string | null
  base_url: string | null
  auth_method: string
  endpoints: PresetEndpoint[]
  category: string
  description: string | null
}

export const listProviders = (page = 1, pageSize = 20, category?: string) =>
  client.get('/api/providers', { params: { page, size: pageSize, ...(category ? { category } : {}) } })
export const exportProviders = () => client.get('/api/providers/export')
export const getProvider = (id: number) => client.get(`/api/providers/${id}`)
export const createProvider = (data: any) => client.post('/api/providers', data)
export const updateProvider = (id: number, data: any) => client.put(`/api/providers/${id}`, data)
export const deleteProvider = (id: number) => client.delete(`/api/providers/${id}`)
export const activateProvider = (id: number) => client.post(`/api/providers/${id}/activate`)
export const deactivateProvider = (id: number) => client.post(`/api/providers/${id}/deactivate`)
export const getProviderPresets = () => client.get('/api/provider-presets')
export const testProvider = (id: number, model?: string) => client.post(`/api/providers/${id}/test`, model ? { model } : undefined)
export const importProviders = (data: any) => client.post('/api/providers/import', data)
export const resetCircuitBreaker = (id: number) => client.post(`/api/proxy/circuit-breaker/${id}/reset`)
export const fetchProviderModels = (id: number) => client.get(`/api/providers/${id}/fetch-models`)
export const fetchModelsAdhoc = (params: { base_url: string; api_key: string; provider_type?: string }) =>
  client.post('/api/providers/fetch-models', params)

// Provider Endpoints
export interface ProviderEndpointItem {
  id: number
  provider_id: number
  endpoint_type: string
  api_format: string
  endpoint_path: string
  http_method: string
  enabled: boolean
  created_at: number
  updated_at: number
}

export const listProviderEndpoints = (providerId: number) =>
  client.get(`/api/providers/${providerId}/endpoints`)
export const createProviderEndpoint = (providerId: number, data: { endpoint_type: string; api_format: string; endpoint_path: string }) =>
  client.post(`/api/providers/${providerId}/endpoints`, data)
export const updateProviderEndpoint = (id: number, data: any) =>
  client.put(`/api/provider-endpoints/${id}`, data)
export const deleteProviderEndpoint = (id: number) =>
  client.delete(`/api/provider-endpoints/${id}`)
