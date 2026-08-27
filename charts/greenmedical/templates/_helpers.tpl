{{/*
Expand the name of the chart.
*/}}
{{- define "greenmedical.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Create a default fully qualified app name (truncated to 63 chars, DNS compatible).
*/}}
{{- define "greenmedical.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{/*
Chart label value.
*/}}
{{- define "greenmedical.chart" -}}
{{- printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Common labels (all resources).
*/}}
{{- define "greenmedical.labels" -}}
helm.sh/chart: {{ include "greenmedical.chart" . }}
{{ include "greenmedical.selectorLabels" . }}
{{- if .Chart.AppVersion }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
{{- end }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/part-of: greenmedical
{{- with .Values.commonLabels }}
{{ toYaml . }}
{{- end }}
{{- end }}

{{/*
Selector labels shared by all components.
*/}}
{{- define "greenmedical.selectorLabels" -}}
app.kubernetes.io/name: {{ include "greenmedical.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{/*
Component names.
*/}}
{{- define "greenmedical.backend.fullname" -}}
{{- printf "%s-backend" (include "greenmedical.fullname" .) | trunc 63 | trimSuffix "-" }}
{{- end }}

{{- define "greenmedical.frontend.fullname" -}}
{{- printf "%s-frontend" (include "greenmedical.fullname" .) | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Component labels / selector labels.
*/}}
{{- define "greenmedical.backend.labels" -}}
{{ include "greenmedical.labels" . }}
app.kubernetes.io/component: backend
{{- end }}

{{- define "greenmedical.backend.selectorLabels" -}}
{{ include "greenmedical.selectorLabels" . }}
app.kubernetes.io/component: backend
{{- end }}

{{- define "greenmedical.frontend.labels" -}}
{{ include "greenmedical.labels" . }}
app.kubernetes.io/component: frontend
{{- end }}

{{- define "greenmedical.frontend.selectorLabels" -}}
{{ include "greenmedical.selectorLabels" . }}
app.kubernetes.io/component: frontend
{{- end }}

{{- define "greenmedical.test.selectorLabels" -}}
{{ include "greenmedical.selectorLabels" . }}
app.kubernetes.io/component: test
{{- end }}

{{/*
Image reference. Usage: include "greenmedical.image" (dict "image" .Values.backend.image "chart" .Chart)
*/}}
{{- define "greenmedical.image" -}}
{{- $tag := .image.tag | default .chart.AppVersion -}}
{{- if .image.digest -}}
{{- printf "%s:%s@%s" .image.repository $tag .image.digest -}}
{{- else -}}
{{- printf "%s:%s" .image.repository $tag -}}
{{- end -}}
{{- end }}

{{/*
ServiceAccount name.
*/}}
{{- define "greenmedical.serviceAccountName" -}}
{{- if .Values.serviceAccount.create }}
{{- default (include "greenmedical.fullname" .) .Values.serviceAccount.name }}
{{- else }}
{{- default "default" .Values.serviceAccount.name }}
{{- end }}
{{- end }}

{{/*
Values validation. Included from the backend Deployment so every render runs it.
*/}}
{{- define "greenmedical.validate" -}}
{{- if and (empty .Values.database.existingSecret) (empty .Values.database.url) }}
{{- fail "greenmedical: a database is required – set database.existingSecret (recommended) or database.url (development only)" }}
{{- end }}
{{- if and .Values.database.existingSecret .Values.database.url }}
{{- fail "greenmedical: set only one of database.existingSecret and database.url" }}
{{- end }}
{{- if and .Values.database.existingSecret (empty .Values.database.existingSecretKey) }}
{{- fail "greenmedical: database.existingSecretKey must not be empty" }}
{{- end }}
{{- if and .Values.adminToken.existingSecret .Values.adminToken.value }}
{{- fail "greenmedical: set only one of adminToken.existingSecret and adminToken.value" }}
{{- end }}
{{- if and .Values.adminToken.existingSecret (empty .Values.adminToken.existingSecretKey) }}
{{- fail "greenmedical: adminToken.existingSecretKey must not be empty" }}
{{- end }}
{{- if and .Values.email.enabled (empty .Values.email.smtp.host) }}
{{- fail "greenmedical: email.enabled requires email.smtp.host" }}
{{- end }}
{{- if not (has .Values.email.smtp.tls (list "starttls" "tls" "none")) }}
{{- fail "greenmedical: email.smtp.tls must be one of starttls, tls, none" }}
{{- end }}
{{- if and .Values.email.smtp.existingSecret (or .Values.email.smtp.username .Values.email.smtp.password) }}
{{- fail "greenmedical: set only one of email.smtp.existingSecret and email.smtp.username/password" }}
{{- end }}
{{- if and (empty .Values.backend.config.publicUrl) (not (and .Values.ingress.enabled .Values.ingress.hosts)) }}
{{- fail "greenmedical: backend.config.publicUrl is required (links in e-mails) unless ingress.enabled with at least one host, from which it is derived" }}
{{- end }}
{{- if lt (int .Values.backend.config.databaseMaxConnections) 4 }}
{{- fail "greenmedical: backend.config.databaseMaxConnections must be >= 4 (one connection is held by the scrape advisory lock)" }}
{{- end }}
{{- /* Without a DB egress rule the backend can never pass /readyz and the rollout hangs silently. */}}
{{- if and .Values.networkPolicy.enabled (empty .Values.networkPolicy.extraEgressCIDRs) (empty .Values.networkPolicy.dbPodSelector) (empty .Values.networkPolicy.backend.extraEgress) }}
{{- fail "greenmedical: networkPolicy.enabled requires a route to PostgreSQL – set networkPolicy.extraEgressCIDRs (external DB) and/or networkPolicy.dbPodSelector (in-cluster DB, e.g. CNPG), or add a rule via networkPolicy.backend.extraEgress" }}
{{- end }}
{{- /* An HPA without metrics is rejected by the API server. */}}
{{- range $name, $component := dict "backend" .Values.backend "frontend" .Values.frontend }}
{{- if and $component.autoscaling.enabled (not $component.autoscaling.targetCPUUtilizationPercentage) (not $component.autoscaling.targetMemoryUtilizationPercentage) }}
{{- fail (printf "greenmedical: %s.autoscaling.enabled requires targetCPUUtilizationPercentage and/or targetMemoryUtilizationPercentage" $name) }}
{{- end }}
{{- end }}
{{- end }}

{{/*
"true" when an optional scalar value is set – i.e. neither nil nor the empty string. Unlike `with`/`if`
this keeps numeric 0 (e.g. scrapeRetryTotal: 0 disables retries).
Usage: {{- if include "greenmedical.isSet" .Values.backend.config.scrapeRetryTotal }}
*/}}
{{- define "greenmedical.isSet" -}}
{{- if and (not (kindIs "invalid" .)) (ne (toString .) "") }}true{{ end -}}
{{- end }}

{{/*
Chart-managed Secret: created when database.url, adminToken.value or inline SMTP credentials are given.
*/}}
{{- define "greenmedical.secret.create" -}}
{{- if or .Values.database.url .Values.adminToken.value (eq (include "greenmedical.smtp.inlineCredentials" .) "true") }}true{{ else }}false{{ end }}
{{- end }}

{{/*
Public base URL for links in e-mails: backend.config.publicUrl, else derived from the first Ingress host.
*/}}
{{- define "greenmedical.publicUrl" -}}
{{- if .Values.backend.config.publicUrl }}
{{- .Values.backend.config.publicUrl | trimSuffix "/" }}
{{- else }}
{{- $host := (index .Values.ingress.hosts 0).host }}
{{- printf "%s://%s" (ternary "https" "http" (not (empty .Values.ingress.tls))) $host }}
{{- end }}
{{- end }}

{{/*
SMTP credentials. "inlineCredentials" = username/password given in values (rendered into the chart Secret);
"credentials" = any credential source configured (existingSecret or inline) while e-mail is enabled.
*/}}
{{- define "greenmedical.smtp.inlineCredentials" -}}
{{- if and .Values.email.enabled (or .Values.email.smtp.username .Values.email.smtp.password) }}true{{ else }}false{{ end }}
{{- end }}

{{- define "greenmedical.smtp.credentials" -}}
{{- if and .Values.email.enabled (or .Values.email.smtp.existingSecret .Values.email.smtp.username .Values.email.smtp.password) }}true{{ else }}false{{ end }}
{{- end }}

{{- define "greenmedical.smtp.secretName" -}}
{{- if .Values.email.smtp.existingSecret }}
{{- .Values.email.smtp.existingSecret }}
{{- else }}
{{- include "greenmedical.secret.name" . }}
{{- end }}
{{- end }}

{{- define "greenmedical.secret.name" -}}
{{- printf "%s-backend" (include "greenmedical.fullname" .) | trunc 63 | trimSuffix "-" }}
{{- end }}

{{/*
Database secret reference.
*/}}
{{- define "greenmedical.database.secretName" -}}
{{- if .Values.database.existingSecret }}
{{- .Values.database.existingSecret }}
{{- else }}
{{- include "greenmedical.secret.name" . }}
{{- end }}
{{- end }}

{{- define "greenmedical.database.secretKey" -}}
{{- if .Values.database.existingSecret }}
{{- .Values.database.existingSecretKey }}
{{- else }}
{{- "DATABASE_URL" }}
{{- end }}
{{- end }}

{{/*
Admin token reference.
*/}}
{{- define "greenmedical.adminToken.enabled" -}}
{{- if or .Values.adminToken.existingSecret .Values.adminToken.value }}true{{ else }}false{{ end }}
{{- end }}

{{- define "greenmedical.adminToken.secretName" -}}
{{- if .Values.adminToken.existingSecret }}
{{- .Values.adminToken.existingSecret }}
{{- else }}
{{- include "greenmedical.secret.name" . }}
{{- end }}
{{- end }}

{{- define "greenmedical.adminToken.secretKey" -}}
{{- if .Values.adminToken.existingSecret }}
{{- .Values.adminToken.existingSecretKey }}
{{- else }}
{{- "ADMIN_TOKEN" }}
{{- end }}
{{- end }}

{{/*
Effective replica count of a component (minReplicas when autoscaling).
Usage: include "greenmedical.replicas" .Values.backend
*/}}
{{- define "greenmedical.replicas" -}}
{{- if .autoscaling.enabled }}{{ .autoscaling.minReplicas }}{{ else }}{{ .replicaCount }}{{ end }}
{{- end }}

{{/*
Render a NetworkPolicy peer list entry from {namespaceSelector, podSelector}. Empty selectors are
dropped; if both are empty the peer matches every pod in every namespace.
Usage: include "greenmedical.networkPolicy.peer" .Values.networkPolicy.ingressController
*/}}
{{- define "greenmedical.networkPolicy.peer" -}}
{{- $peer := dict -}}
{{- if or .namespaceSelector .podSelector -}}
{{- with .namespaceSelector }}{{ $_ := set $peer "namespaceSelector" . }}{{ end -}}
{{- with .podSelector }}{{ $_ := set $peer "podSelector" . }}{{ end -}}
{{- else -}}
{{- $_ := set $peer "namespaceSelector" dict -}}
{{- end -}}
{{- toYaml (list $peer) -}}
{{- end }}

{{/*
Render an HPA spec body. Usage: include "greenmedical.hpa" (dict "component" .Values.backend "name" (include "greenmedical.backend.fullname" .))
*/}}
{{- define "greenmedical.hpa.spec" -}}
scaleTargetRef:
  apiVersion: apps/v1
  kind: Deployment
  name: {{ .name }}
minReplicas: {{ .component.autoscaling.minReplicas }}
maxReplicas: {{ .component.autoscaling.maxReplicas }}
metrics:
  {{- if .component.autoscaling.targetCPUUtilizationPercentage }}
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: {{ .component.autoscaling.targetCPUUtilizationPercentage }}
  {{- end }}
  {{- if .component.autoscaling.targetMemoryUtilizationPercentage }}
  - type: Resource
    resource:
      name: memory
      target:
        type: Utilization
        averageUtilization: {{ .component.autoscaling.targetMemoryUtilizationPercentage }}
  {{- end }}
{{- with .component.autoscaling.behavior }}
behavior:
  {{- toYaml . | nindent 2 }}
{{- end }}
{{- end }}

{{/*
Render a PDB spec body. Usage: include "greenmedical.pdb.spec" .Values.backend
*/}}
{{- define "greenmedical.pdb.spec" -}}
{{- if .podDisruptionBudget.maxUnavailable }}
maxUnavailable: {{ .podDisruptionBudget.maxUnavailable }}
{{- else }}
minAvailable: {{ .podDisruptionBudget.minAvailable }}
{{- end }}
{{- end }}
