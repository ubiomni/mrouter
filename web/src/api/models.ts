import client from './client'

export const listProviderModels = (providerId: number) => client.get(`/api/providers/${providerId}/models`)
export const createProviderModel = (providerId: number, data: any) => client.post(`/api/providers/${providerId}/models`, data)
export const updateProviderModel = (id: number, data: any) => client.put(`/api/provider-models/${id}`, data)
export const deleteProviderModel = (id: number) => client.delete(`/api/provider-models/${id}`)
export const listModelsMetadata = () => client.get('/api/models/metadata')
export const upsertModelMetadata = (data: any) => client.post('/api/models/metadata', data)
export const deleteModelMetadata = (modelName: string) => client.delete(`/api/models/metadata/${modelName}`)
export const listAvailableModels = () => client.get('/api/provider-models/available')
