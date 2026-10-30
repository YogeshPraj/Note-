'use strict';
// =============================================================================
// Note++ — Developer-friendly word list
// =============================================================================
// The base en_US Hunspell dictionary doesn't know everyday developer
// vocabulary — `autoscale`, `kubectl`, `webhook`, `OAuth`, etc. — so it
// flags them as misspelled, which is more noise than signal.
//
// This list seeds nspell on startup so the standard tech vocabulary is
// recognised out of the box. Curated conservatively: every entry has to
// be (a) actually used in modern dev, (b) unlikely to mask a misspelling
// of a real English word.
//
// All entries are lowercase — nspell's `correct()` lowercases input
// before comparing, so casing doesn't matter for matching. Acronyms are
// included in lowercase so both `API` and `api` pass.
// =============================================================================

module.exports = [
  // ── Cloud / containers / orchestration ──────────────────────────────
  'autoscale', 'autoscaler', 'autoscaling',
  'kubectl', 'kubeadm', 'kubelet', 'kubeconfig', 'kubernetes', 'kube', 'k8s', 'k3s',
  'helm', 'helmfile', 'tiller',
  'terraform', 'terragrunt', 'pulumi',
  'ansible', 'puppet', 'chef', 'saltstack',
  'docker', 'dockerfile', 'dockerignore', 'docker-compose', 'compose',
  'podman', 'containerd', 'runc', 'crio',
  'istio', 'envoy', 'linkerd', 'consul',
  'prometheus', 'grafana', 'alertmanager', 'pushgateway',
  'jaeger', 'zipkin', 'opentelemetry', 'otel',
  'vault', 'nomad', 'packer', 'vagrant', 'minikube',
  'argo', 'argocd', 'flux', 'fluxcd', 'spinnaker',
  'traefik', 'nginx', 'haproxy', 'caddy', 'apache',
  'cni', 'csi', 'sidecar', 'daemonset', 'statefulset', 'replicaset',

  // ── Cloud providers — AWS ────────────────────────────────────────────
  'aws', 'ec2', 'ec2s', 's3', 'rds', 'dynamodb', 'lambda', 'lambdas',
  'eks', 'ecs', 'ecr', 'fargate',
  'cloudwatch', 'cloudtrail', 'cloudfront', 'cloudformation',
  'iam', 'kms', 'vpc', 'vpcs', 'alb', 'elb', 'nlb',
  'sqs', 'sns', 'ses', 'sfn', 'eventbridge',
  'redshift', 'athena', 'glue', 'kinesis', 'firehose',
  'codebuild', 'codedeploy', 'codepipeline', 'codecommit',
  'amplify', 'cognito', 'appsync', 'appflow',
  'cloudshell', 'cloudwatch', 'route53',

  // ── Cloud providers — GCP / Azure ────────────────────────────────────
  'gcp', 'gke', 'gcr', 'gcs', 'bigquery', 'bigtable',
  'dataflow', 'pubsub', 'firestore', 'firebase', 'firestoredb',
  'azure', 'aks', 'azureml', 'cosmosdb', 'eventhub',

  // ── Web protocols / formats ──────────────────────────────────────────
  'http', 'https', 'websocket', 'websockets',
  'grpc', 'graphql', 'rest', 'restful', 'soap',
  'jsonrpc', 'xmlrpc',
  'cors', 'csrf', 'xss', 'csp', 'hsts', 'sri',
  'oauth', 'oauth2', 'oidc', 'jwt', 'saml', 'ldap',
  'mfa', '2fa', 'totp', 'hotp',
  'json', 'jsonc', 'yaml', 'toml', 'csv', 'tsv', 'edn',
  'xml', 'html', 'svg', 'mathml', 'jsx', 'tsx', 'mdx', 'markdown',
  'xpath', 'xslt', 'xsd', 'rngs',
  'webhook', 'webhooks', 'longpoll', 'sse',
  'cdn', 'edge', 'pop', 'tld',

  // ── Programming concepts ─────────────────────────────────────────────
  'async', 'await', 'promise', 'callback', 'callbacks', 'coroutine', 'goroutine',
  'lambda', 'closure', 'currying', 'monad', 'functor',
  'stdin', 'stdout', 'stderr', 'ipc', 'syscall',
  'malloc', 'calloc', 'dealloc', 'sizeof', 'realloc',
  'regex', 'regexp',
  'mutex', 'semaphore', 'condvar', 'deadlock', 'livelock', 'rwlock',
  'enum', 'enums', 'struct', 'structs', 'typedef', 'interface', 'trait', 'traits',
  'nullable', 'nonnull', 'optional', 'maybe',
  'args', 'kwargs', 'varargs', 'params', 'getter', 'setter',
  'ctor', 'dtor',
  'runtime', 'runtimes', 'compile-time', 'codegen',
  'tokenize', 'tokenizer', 'parser', 'lexer', 'lexed', 'ast',
  'bytecode', 'opcode', 'opcodes',
  'memoize', 'memoise', 'memoized',
  'backtrack', 'backtracks', 'backtracking',
  'iterator', 'iterators', 'iterable', 'iterables',
  'deserialize', 'serialize', 'serializer', 'deserializer', 'marshal', 'unmarshal',
  'enqueue', 'dequeue', 'pubsub',
  'debounce', 'throttle', 'throttled', 'debounced',
  'minify', 'minified', 'minifier', 'transpile', 'transpiler', 'polyfill', 'polyfills',
  'tokenize', 'tokenizes', 'tokenized',

  // ── Common dev compound words ────────────────────────────────────────
  'backend', 'frontend', 'fullstack',
  'middleware', 'middlewares',
  'hostname', 'dirname', 'basename', 'pathname', 'filename',
  'timestamp', 'timestamps', 'datetime',
  'fallback', 'lookup', 'lookups',
  'namespace', 'namespaces', 'codebase', 'codebases',
  'workflow', 'workflows', 'workspace', 'workspaces',
  'changelog', 'gitignore', 'makefile', 'dockerfile',
  'signup', 'signin', 'login', 'logout', 'signout',
  'subdomain', 'subdomains',
  'snapshot', 'snapshots',
  'gzip', 'gzipped', 'bzip', 'brotli', 'zstd',
  'offline', 'online', 'onload', 'onclick', 'onsubmit', 'onchange', 'onhover', 'ondrop',
  'failover', 'rollback', 'rollbacks', 'rollout', 'rollouts',
  'toolchain', 'toolchains',
  'multiline', 'multiarch', 'multitenant', 'multicluster',
  'preview', 'previews', 'prebuild', 'prebuilt', 'postbuild',
  'prerelease', 'postinstall', 'preinstall',
  'monorepo', 'monorepos', 'polyrepo',
  'dotfile', 'dotfiles', 'npmrc',
  'redeploy', 'redeploys',
  'config', 'configs', 'configmap', 'configmaps',
  'env', 'envs', 'envvars',
  'devops', 'gitops', 'mlops', 'finops', 'secops',
  'sso', 'rbac', 'abac', 'acl', 'acls',
  'mtls', 'tls', 'ssl', 'pki',

  // ── Build tools / frameworks ─────────────────────────────────────────
  'github', 'gitlab', 'bitbucket', 'jira', 'trello', 'asana', 'notion',
  'npm', 'yarn', 'pnpm', 'deno', 'bun', 'nodejs',
  'webpack', 'vite', 'esbuild', 'rollup', 'parcel', 'snowpack', 'turbopack',
  'eslint', 'tslint', 'prettier', 'stylelint', 'husky',
  'mocha', 'chai', 'jest', 'vitest', 'jasmine', 'karma', 'cypress', 'playwright', 'puppeteer', 'selenium',
  'babel', 'swc', 'tsc', 'typescript', 'javascript',
  'jquery', 'lodash', 'ramda', 'immer', 'immutable',
  'react', 'vue', 'svelte', 'solid', 'angular', 'preact', 'lit', 'stencil',
  'nextjs', 'nuxt', 'gatsby', 'eleventy', 'astro', 'remix',
  'redux', 'mobx', 'vuex', 'pinia', 'zustand', 'recoil', 'jotai',
  'expressjs', 'express', 'fastify', 'hapi', 'koa', 'nestjs',
  'django', 'flask', 'fastapi', 'starlette', 'tornado', 'aiohttp',
  'sinatra', 'rails',
  'spring', 'springboot',
  'dotnet', 'aspnet', 'blazor',
  'laravel', 'symfony', 'codeigniter',
  'hugo', 'jekyll', 'pelican',
  'hibernate', 'mongoose', 'sequelize', 'typeorm', 'prisma', 'knex', 'drizzle',
  'pandas', 'numpy', 'scipy', 'scikit', 'sklearn', 'pytorch', 'tensorflow', 'keras', 'jax',
  'gunicorn', 'uvicorn', 'uwsgi', 'supervisord', 'systemd',
  'pytest', 'unittest', 'junit', 'testng',

  // ── Databases / data stores ──────────────────────────────────────────
  'postgres', 'postgresql', 'mysql', 'mariadb', 'mongodb', 'mongo',
  'redis', 'memcached', 'sqlite',
  'elasticsearch', 'opensearch', 'solr', 'lucene',
  'cassandra', 'scylla', 'cockroachdb', 'crdb',
  'couchdb', 'couchbase', 'neo4j', 'rethinkdb',
  'clickhouse', 'duckdb', 'snowflake', 'databricks',
  'sql', 'nosql', 'newsql', 'oltp', 'olap', 'cte', 'ddl', 'dml',

  // ── Acronyms (lowercase forms for casefold matching) ─────────────────
  'api', 'apis', 'sdk', 'sdks', 'ide', 'cli', 'gui', 'tui', 'url', 'urls', 'uri', 'uris',
  'dns', 'tls', 'ssl',
  'tcp', 'udp', 'icmp', 'ip', 'ipv4', 'ipv6',
  'smtp', 'pop3', 'imap', 'ftp', 'sftp', 'scp', 'ssh',
  'ui', 'ux', 'db', 'dba', 'qa', 'qc', 'sre',
  'ci', 'cd', 'cicd', 'vcs', 'scm',
  'ssd', 'hdd', 'nvme', 'ram', 'rom', 'cpu', 'gpu', 'tpu', 'npu',
  'ssr', 'csr', 'ssg', 'isr', 'spa', 'pwa', 'lts',
  'io', 'fifo', 'lifo', 'fpga', 'asic', 'soc',
  'csv', 'tsv', 'tsvs',
  'qps', 'rps', 'tps', 'rpm', 'lps',
  'mvp', 'poc', 'mvc', 'mvvm', 'orm', 'rdbms',

  // ── Programming languages ────────────────────────────────────────────
  'golang', 'kotlin', 'scala', 'rust', 'rustlang',
  'elixir', 'erlang', 'haskell', 'ocaml', 'fsharp', 'csharp',
  'typescript', 'javascript', 'python', 'pythonic',
  'ruby', 'php', 'perl', 'lua',
  'clojure', 'clojurescript', 'racket', 'scheme',
  'dart', 'flutter',
  'zig', 'nim', 'crystal', 'odin',

  // ── Software lifecycle / environments ────────────────────────────────
  'prod', 'dev', 'qa', 'uat', 'staging',
  'alpha', 'beta', 'rc',
  'lts', 'sla', 'slo', 'sli', 'sli', 'mttd', 'mttr', 'rto', 'rpo',

  // ── Linux / Unix / shell ─────────────────────────────────────────────
  'sudo', 'chmod', 'chown', 'chroot', 'mkdir', 'rmdir', 'mkfs',
  'grep', 'awk', 'sed', 'curl', 'wget', 'jq', 'yq', 'fzf', 'ripgrep',
  'bash', 'zsh', 'fish', 'tmux', 'nvim', 'neovim', 'vim', 'emacs',
  'systemd', 'cron', 'crontab', 'inetd',
  'iptables', 'nftables', 'firewalld', 'selinux', 'apparmor',
  'cgroup', 'cgroups', 'namespaces', 'unshare',
  'ext4', 'btrfs', 'xfs', 'zfs', 'tmpfs', 'overlayfs',

  // ── Git terminology ──────────────────────────────────────────────────
  'gitignore', 'gitattributes',
  'rebase', 'rebased', 'rebasing',
  'cherrypick', 'cherrypicked',
  'fastforward', 'noff',
  'submodule', 'submodules', 'subtree',
  'upstream', 'downstream',
  'reflog', 'stash', 'stashed', 'stashes',
  'commit-ish', 'committish',

  // ── Crypto / security ────────────────────────────────────────────────
  'argon2', 'bcrypt', 'scrypt', 'pbkdf2', 'hkdf', 'hmac',
  'sha1', 'sha256', 'sha512', 'md5',
  'aes', 'rsa', 'ed25519', 'ecdsa', 'eddsa', 'x25519',
  'gcm', 'cbc', 'cfb', 'ctr',
  'salt', 'salted', 'nonce', 'nonces',
  'pem', 'der', 'pkcs', 'pfx',
  'csrf', 'csp', 'xss', 'sqli', 'rce', 'ssrf',

  // ── Common AI / ML terms ─────────────────────────────────────────────
  'llm', 'llms', 'gpt', 'rag', 'embedding', 'embeddings',
  'tokenizer', 'inference', 'finetune', 'finetuning', 'finetuned',
  'quantize', 'quantization', 'quantized', 'distill', 'distilled',
  'prompt', 'prompts', 'prompting', 'few-shot', 'zero-shot',
  'transformer', 'transformers', 'softmax', 'logit', 'logits',
  'backprop', 'backpropagation', 'gradient',
  'cnn', 'rnn', 'lstm', 'gru', 'mlp', 'gan', 'vae',
  'huggingface', 'pytorch', 'tensorflow',

  // ── File extensions / dotfiles ───────────────────────────────────────
  'js', 'ts', 'jsx', 'tsx', 'mjs', 'cjs',
  'py', 'pyc', 'rb', 'rs', 'go', 'java', 'kt', 'kts',
  'rc', 'eslintrc', 'prettierrc', 'babelrc', 'editorconfig',
];
