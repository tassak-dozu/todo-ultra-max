FROM oven/bun:1-alpine

WORKDIR /app

COPY package.json ./
COPY src ./src

ENV PORT=3000
ENV DB_PATH=/data/todos.db

VOLUME ["/data"]
EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD bun -e "fetch('http://127.0.0.1:3000/api/health').then(r => process.exit(r.ok ? 0 : 1)).catch(() => process.exit(1))"

CMD ["bun", "run", "src/index.ts"]
