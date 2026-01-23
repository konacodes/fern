# DevPulse - Developer Productivity Analytics Platform

![DevPulse](https://img.shields.io/badge/DevPulse-Analytics-blue)
![Python](https://img.shields.io/badge/Python-3.11+-green)
![TypeScript](https://img.shields.io/badge/TypeScript-5.0+-blue)
![FastAPI](https://img.shields.io/badge/FastAPI-0.104+-teal)
![React](https://img.shields.io/badge/React-18+-purple)

DevPulse is a comprehensive developer productivity analytics platform that monitors coding activity, analyzes git repositories, and provides actionable insights through a beautiful web dashboard and CLI tool.

## Features

- **Git Repository Analysis**: Automatically analyze commits, languages, and code changes
- **Real-time Dashboard**: Beautiful web interface with live updates via WebSocket
- **CLI Tool**: Quick stats and management from the command line
- **Productivity Metrics**: Track commits, lines of code, language distribution, and more
- **Session Tracking**: Monitor coding sessions with start/stop functionality
- **Reports**: Generate weekly and monthly productivity reports
- **Multi-Repository**: Track multiple repositories simultaneously

## Tech Stack

### Backend
- **Framework**: FastAPI (Python 3.11+)
- **Database**: SQLite (upgradeable to PostgreSQL)
- **ORM**: SQLAlchemy with Alembic migrations
- **Authentication**: JWT-based
- **Git Analysis**: GitPython
- **WebSocket**: FastAPI WebSocket support

### Frontend
- **Runtime**: Bun
- **Framework**: React 18 with TypeScript
- **Build Tool**: Vite
- **Styling**: TailwindCSS
- **Charts**: Recharts
- **Real-time**: WebSocket client

### CLI
- **Framework**: Click (Python)
- **API Client**: HTTP requests to backend

### DevOps
- **Containerization**: Docker + docker-compose
- **Testing**: pytest (backend), vitest (frontend)

## Quick Start

### Prerequisites

- Python 3.11+
- Bun (or Node.js 18+)
- Docker & docker-compose (optional)
- Git

### Installation

#### 1. Clone the Repository

```bash
git clone <repository-url>
cd devpulse
```

#### 2. Set Up Environment Variables

```bash
cp .env.example .env
# Edit .env with your configuration
```

#### 3. Using Docker (Recommended)

```bash
docker-compose up --build
```

- Frontend: http://localhost:3000
- Backend API: http://localhost:8000
- API Docs: http://localhost:8000/docs

#### 4. Manual Setup

**Backend:**

```bash
cd backend
python -m venv venv
source venv/bin/activate  # On Windows: venv\Scripts\activate
pip install -r requirements.txt
alembic upgrade head
uvicorn app.main:app --reload
```

**Frontend:**

```bash
cd frontend
bun install
bun run dev
```

**CLI:**

```bash
cd cli
pip install -e .
devpulse --help
```

## Usage

### Web Dashboard

1. Navigate to http://localhost:3000
2. Register/Login
3. Add repositories via the UI
4. View analytics and metrics in real-time

### CLI Commands

```bash
# Show quick statistics
devpulse stats

# Add a repository
devpulse repo add /path/to/repository

# List repositories
devpulse repo list

# Analyze a repository
devpulse repo analyze my-repo

# Start coding session
devpulse session start

# Stop coding session
devpulse session stop

# Generate reports
devpulse report weekly
devpulse report monthly
```

### API Endpoints

Full API documentation available at: http://localhost:8000/docs

**Authentication:**
- POST `/api/auth/register` - Register new user
- POST `/api/auth/login` - Login and get JWT token

**Repositories:**
- POST `/api/repositories` - Add repository
- GET `/api/repositories` - List repositories
- GET `/api/repositories/{id}` - Get repository details
- DELETE `/api/repositories/{id}` - Remove repository
- POST `/api/repositories/{id}/analyze` - Trigger analysis

**Metrics:**
- GET `/api/metrics/daily` - Daily metrics
- GET `/api/metrics/weekly` - Weekly summary
- GET `/api/metrics/monthly` - Monthly summary
- GET `/api/metrics/languages` - Language breakdown

**Sessions:**
- POST `/api/sessions/start` - Start coding session
- POST `/api/sessions/stop` - Stop coding session
- GET `/api/sessions` - List sessions

**WebSocket:**
- WS `/ws/updates` - Real-time updates stream

## Development

### Running Tests

**Backend:**
```bash
cd backend
pytest --cov=app --cov-report=html
```

**Frontend:**
```bash
cd frontend
bun test --coverage
```

**CLI:**
```bash
cd cli
pytest --cov=devpulse
```

### Database Migrations

```bash
cd backend
alembic revision --autogenerate -m "Description"
alembic upgrade head
```

## Project Structure

```
devpulse/
├── backend/           # FastAPI backend
│   ├── app/          # Application code
│   ├── analyzer/     # Git analysis logic
│   ├── migrations/   # Alembic migrations
│   └── tests/        # Backend tests
├── frontend/         # React frontend
│   ├── src/         # Source code
│   └── tests/       # Frontend tests
├── cli/             # CLI tool
│   ├── devpulse/   # CLI code
│   └── tests/      # CLI tests
├── docker-compose.yml
├── .env.example
├── SPEC.md          # Implementation specification
└── README.md
```

## Real-World Benefits

- **Self-awareness**: Understand when you're most productive
- **Language insights**: See which languages/frameworks you use most
- **Project tracking**: Monitor activity across multiple repositories
- **Team insights**: Optional multi-user support for team analytics
- **Habit formation**: Identify productive patterns and areas for improvement
- **Portfolio data**: Generate reports for showcasing your development activity

## Contributing

See SPEC.md for the detailed implementation specification and checklist.

## License

MIT License - see LICENSE file for details

## Support

For issues and questions, please open an issue on GitHub.

---

Built with ❤️ using FastAPI, React, and Bun
