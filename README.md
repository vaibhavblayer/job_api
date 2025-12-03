# Job Application API

A comprehensive job application platform built with Rust, featuring a REST API, CLI tool, and end-to-end testing suite.

## 🚀 Features

### Core Modules

1. **Authentication & Admin Management**
   - Google OAuth authentication
   - JWT token-based authorization
   - Admin user management
   - Role-based access control

2. **Job Management**
   - Create, update, and delete job postings
   - Job status workflow (draft → active → closed)
   - Featured jobs
   - Bulk operations
   - Job analytics and tracking

3. **Application Processing**
   - Submit job applications
   - Resume upload and management
   - Application status tracking (submitted → reviewed → interviewing → offered → hired)
   - Admin application review
   - Bulk status updates
   - Application analytics

### Additional Features

- **Profile Management**: User profiles, experience, education, testimonials
- **Messaging System**: Real-time messaging with WebSocket support
- **Admin Dashboard**: Comprehensive metrics and analytics
- **Companies Module**: Company profiles and assets
- **Dev Mode**: Development mode for testing without authentication

## 📁 Project Structure

```
.
├── src/                    # Main API server
│   ├── auth/              # Authentication handlers
│   ├── jobs/              # Job management
│   ├── candidates/        # Applications & resumes
│   ├── profile/           # User profiles
│   ├── messages/          # Messaging system
│   ├── admin/             # Admin dashboard
│   ├── companies/         # Company management
│   └── common/            # Shared utilities
├── jobcli/                # CLI tool
├── e2e-tests/             # End-to-end test suite
├── postman/               # Postman collections
└── scripts/               # Utility scripts
```

## 🛠️ Setup

### Prerequisites

- Rust 1.70+
- SQLite 3
- Google OAuth credentials (for authentication)

### Installation

1. Clone the repository
2. Copy `.env.test` to `.env` and configure:
   ```bash
   cp .env.test .env
   ```

3. Set up Google OAuth:
   - Follow `scripts/setup-google-oauth.md`
   - Add credentials to `.env`

4. Build the project:
   ```bash
   cargo build --release
   ```

### Running the API Server

```bash
# Development mode (no auth required)
DEV_MODE=true cargo run

# Production mode
cargo run --release
```

The server runs on `http://localhost:8080`

## 📚 Documentation

- **[Dev Mode Guide](DEV_MODE_GUIDE.md)** - Testing without authentication
- **[Google Auth Guide](GOOGLE_AUTH_GUIDE.md)** - OAuth setup
- **[CLI Tool](jobcli/README.md)** - Command-line interface
- **[E2E Tests](e2e-tests/README.md)** - Test suite documentation
- **[Postman Collections](postman/README.md)** - API testing

## 🧪 Testing

### Run Unit Tests
```bash
cargo test
```

### Run E2E Tests
```bash
cd e2e-tests
cargo test
```

**Test Status:** 260/275 tests passing (94.5%)

See [e2e-tests/FINAL_TEST_REPORT.md](e2e-tests/FINAL_TEST_REPORT.md) for details.

## 🔧 CLI Tool

The `jobcli` tool provides a command-line interface for the API:

```bash
# Install
cd jobcli
cargo install --path .

# Login
jobcli auth login

# List jobs
jobcli jobs list

# Apply to a job
jobcli jobs apply <job-id>

# View applications
jobcli applications list
```

See [jobcli/README.md](jobcli/README.md) for full documentation.

## 📊 API Endpoints

### Authentication
- `POST /api/auth/google` - Google OAuth login
- `GET /api/me` - Get current user
- `POST /api/auth/logout` - Logout

### Jobs
- `GET /api/jobs` - List jobs (public)
- `GET /api/jobs/:id` - Get job details
- `POST /api/admin/jobs` - Create job (admin)
- `PUT /api/admin/jobs/:id` - Update job (admin)
- `DELETE /api/admin/jobs/:id` - Delete job (admin)

### Applications
- `POST /api/applications` - Submit application
- `GET /api/applications` - List user applications
- `GET /api/applications/:id` - Get application details
- `PATCH /api/applications/:id/status` - Update status (admin)

### Resumes
- `POST /api/resumes` - Upload resume
- `GET /api/user/resumes` - List resumes
- `DELETE /api/resumes/:id` - Delete resume
- `GET /api/resumes/:id/download` - Download resume

### Admin
- `GET /api/admin/dashboard/metrics` - Dashboard metrics
- `GET /api/admin/candidates` - List candidates
- `GET /api/admin/users` - Manage admin users

See [postman/](postman/) for complete API collections.

## 🔐 Security

- JWT-based authentication
- Google OAuth integration
- Admin role enforcement
- Input validation
- SQL injection prevention (using sqlx)

## 🚦 Status

**Production Ready** ✅

All core modules are functional and tested:
- ✅ Authentication & Admin
- ✅ Job Creation & Management
- ✅ Application Processing
- ✅ Resume Management
- ✅ Profile Management
- ✅ Messaging System

## 📝 License

[Add your license here]

## 🤝 Contributing

[Add contribution guidelines here]
