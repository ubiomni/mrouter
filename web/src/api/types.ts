export interface User {
  id: number;
  username: string;
  display_name: string | null;
  role: string;
  org_id: number;
}

export interface LoginResponse {
  token: string;
  user: User;
  expires_in: number;
}

export interface Provider {
  id: number;
  org_id: number;
  name: string;
  display_name: string | null;
  provider_type: string;
  api_format: string;
  base_url: string;
  api_key: string;
  priority: number;
  weight: number;
  provider_group: string;
  rpm_limit: number;
  tpm_limit: number;
  concurrent_limit: number;
  is_active: boolean;
  auto_ban: boolean;
  remark: string | null;
  created_at: string;
  updated_at: string;
}

export interface ApiKey {
  id: number;
  org_id: number;
  user_id: number | null;
  name: string;
  key_hash: string;
  key_prefix: string;
  is_active: boolean;
  expires_at: string | null;
  rpm_limit: number;
  tpm_limit: number;
  allowed_models: string[] | null;
  model_limits: string[] | null;
  created_at: string;
  updated_at: string;
}

export interface Organization {
  id: number;
  name: string;
  display_name: string | null;
  is_active: boolean;
  created_at: string;
  updated_at: string;
}

export interface ProviderModel {
  id: number;
  provider_id: number;
  model_name: string;
  is_active: boolean;
  created_at: string;
  updated_at: string;
}

export interface ModelMetadata {
  id: number;
  model_name: string;
  display_name: string | null;
  provider_type: string;
  input_price: number;
  output_price: number;
  context_window: number;
  max_output: number;
  is_active: boolean;
  created_at: string;
  updated_at: string;
}

export interface SystemSetting {
  id: number;
  key: string;
  value: string;
  description: string | null;
  updated_at: string;
}

export interface PaginatedResponse<T> {
  data: T[];
  total: number;
  page: number;
  page_size: number;
}
