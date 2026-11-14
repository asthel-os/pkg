use std::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Pt,
    En,
    Es,
    Fr,
    De,
    It,
    Ru,
    Ja,
    Zh,
}

impl Lang {
    pub fn detect() -> Self {
        let locale = env::var("LC_ALL")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| env::var("LC_MESSAGES").ok().filter(|s| !s.is_empty()))
            .or_else(|| env::var("LANGUAGE").ok().filter(|s| !s.is_empty()))
            .or_else(|| env::var("LANG").ok().filter(|s| !s.is_empty()))
            .unwrap_or_else(|| "en_US.UTF-8".to_string())
            .to_lowercase();

        let code = locale
            .split(['_', '.', ':', '-'])
            .next()
            .unwrap_or("en");

        match code {
            "pt" => Lang::Pt,
            "es" => Lang::Es,
            "fr" => Lang::Fr,
            "de" => Lang::De,
            "it" => Lang::It,
            "ru" => Lang::Ru,
            "ja" => Lang::Ja,
            "zh" => Lang::Zh,
            _ => Lang::En,
        }
    }

    pub fn sync_done(&self, repos: usize, pkgs: usize) -> String {
        match self {
            Lang::Pt => format!("{repos} repositórios sincronizados • {pkgs} pacotes indexados"),
            Lang::Es => format!("{repos} repositorios sincronizados • {pkgs} paquetes indexados"),
            Lang::Fr => format!("{repos} dépôts synchronisés • {pkgs} paquets indexés"),
            Lang::De => format!("{repos} Repositories synchronisiert • {pkgs} Pakete indexiert"),
            Lang::It => format!("{repos} repository sincronizzati • {pkgs} pacchetti indicizzati"),
            Lang::Ru => format!("Синхронизировано репозиториев: {repos} • пакетов в индексе: {pkgs}"),
            Lang::Ja => format!("{repos} リポジトリ同期完了 • {pkgs} パッケージをインデックス"),
            Lang::Zh => format!("已同步 {repos} 个仓库 • 已索引 {pkgs} 个软件包"),
            Lang::En => format!("{repos} repositories synced • {pkgs} packages indexed"),
        }
    }

    pub fn resolving_deps(&self) -> &'static str {
        match self {
            Lang::Pt => "Resolvendo árvore de dependências e estado do sistema...",
            Lang::Es => "Resolviendo árbol de dependencias y estado del sistema...",
            Lang::Fr => "Résolution de l'arbre des dépendances et de l'état du système...",
            Lang::De => "Abhängigkeitsbaum und Systemstatus werden aufgelöst...",
            Lang::It => "Risoluzione dell'albero delle dipendenze e stato del sistema...",
            Lang::Ru => "Разрешение дерева зависимостей и состояния системы...",
            Lang::Ja => "依存関係ツリーとシステム状態を解決しています...",
            Lang::Zh => "正在解析依赖关系树和系统状态...",
            Lang::En => "Resolving dependency tree and system state...",
        }
    }

    pub fn plan_title_install(&self) -> &'static str {
        match self {
            Lang::Pt => "Plano de Instalação",
            Lang::Es => "Plan de Instalación",
            Lang::Fr => "Plan d'Installation",
            Lang::De => "Installationsplan",
            Lang::It => "Piano di Installazione",
            Lang::Ru => "План установки",
            Lang::Ja => "インストール計画",
            Lang::Zh => "安装计划",
            Lang::En => "Installation Plan",
        }
    }

    pub fn plan_title_remove(&self) -> &'static str {
        match self {
            Lang::Pt => "Plano de Remoção",
            Lang::Es => "Plan de Eliminación",
            Lang::Fr => "Plan de Suppression",
            Lang::De => "Entfernungsplan",
            Lang::It => "Piano di Rimozione",
            Lang::Ru => "План удаления",
            Lang::Ja => "削除計画",
            Lang::Zh => "卸载计划",
            Lang::En => "Removal Plan",
        }
    }

    pub fn target_packages_label(&self, count: usize) -> String {
        match self {
            Lang::Pt => format!("Pacotes principais ({count}):"),
            Lang::Es => format!("Paquetes principales ({count}):"),
            Lang::Fr => format!("Paquets principaux ({count}) :"),
            Lang::De => format!("Hauptpakete ({count}):"),
            Lang::It => format!("Pacchetti principali ({count}):"),
            Lang::Ru => format!("Основные пакеты ({count}):"),
            Lang::Ja => format!("対象パッケージ ({count}):"),
            Lang::Zh => format!("目标软件包 ({count}):"),
            Lang::En => format!("Target packages ({count}):"),
        }
    }

    pub fn dependencies_label(&self, count: usize) -> String {
        match self {
            Lang::Pt => format!("Dependências necessárias ({count}):"),
            Lang::Es => format!("Dependencias requeridas ({count}):"),
            Lang::Fr => format!("Dépendances requises ({count}) :"),
            Lang::De => format!("Erforderliche Abhängigkeiten ({count}):"),
            Lang::It => format!("Dipendenze richieste ({count}):"),
            Lang::Ru => format!("Необходимые зависимости ({count}):"),
            Lang::Ja => format!("必要な依存関係 ({count}):"),
            Lang::Zh => format!("所需依赖项 ({count}):"),
            Lang::En => format!("Required dependencies ({count}):"),
        }
    }

    pub fn reverse_deps_label(&self, count: usize) -> String {
        match self {
            Lang::Pt => format!("Pacotes dependentes que também serão removidos ({count}):"),
            Lang::Es => format!("Paquetes dependientes que también se eliminarán ({count}):"),
            Lang::Fr => format!("Paquets dépendants qui seront aussi supprimés ({count}) :"),
            Lang::De => format!("Abhängige Pakete, die ebenfalls entfernt werden ({count}):"),
            Lang::It => format!("Pacchetti dipendenti che verranno rimossi ({count}):"),
            Lang::Ru => format!("Зависимые пакеты, которые также будут удалены ({count}):"),
            Lang::Ja => format!("同時に削除される依存パッケージ ({count}):"),
            Lang::Zh => format!("将同时卸载的依赖软件包 ({count}):"),
            Lang::En => format!("Dependent packages to also be removed ({count}):"),
        }
    }

    pub fn summary_labels(&self) -> (&'static str, &'static str, &'static str, &'static str) {
        match self {
            Lang::Pt => ("Instalar", "Atualizar", "Remover", "Download"),
            Lang::Es => ("Instalar", "Actualizar", "Eliminar", "Descarga"),
            Lang::Fr => ("Installer", "Mettre à jour", "Supprimer", "Téléchargement"),
            Lang::De => ("Installieren", "Aktualisieren", "Entfernen", "Download"),
            Lang::It => ("Installare", "Aggiornare", "Rimuovere", "Download"),
            Lang::Ru => ("Установка", "Обновление", "Удаление", "Загрузка"),
            Lang::Ja => ("インストール", "更新", "削除", "ダウンロード"),
            Lang::Zh => ("安装", "升级", "卸载", "下载"),
            Lang::En => ("Install", "Upgrade", "Remove", "Download"),
        }
    }

    pub fn disk_impact_label(&self, is_freed: bool) -> &'static str {
        match (self, is_freed) {
            (Lang::Pt, false) => "Uso de disco",
            (Lang::Pt, true) => "Espaço liberado",
            (Lang::Es, false) => "Uso de disco",
            (Lang::Es, true) => "Espacio liberado",
            (Lang::Fr, false) => "Espace utilisé",
            (Lang::Fr, true) => "Espace libéré",
            (Lang::De, false) => "Speicherbedarf",
            (Lang::De, true) => "Freigegeben",
            (Lang::It, false) => "Spazio usato",
            (Lang::It, true) => "Spazio liberato",
            (Lang::Ru, false) => "Место на диске",
            (Lang::Ru, true) => "Освободится",
            (Lang::Ja, false) => "ディスク使用量",
            (Lang::Ja, true) => "解放される容量",
            (Lang::Zh, false) => "磁盘占用",
            (Lang::Zh, true) => "释放空间",
            (Lang::En, false) => "Disk usage",
            (Lang::En, true) => "Disk freed",
        }
    }

    pub fn confirm_prompt(&self) -> &'static str {
        match self {
            Lang::Pt => "Deseja aplicar estas alterações? [S/n]: ",
            Lang::Es => "¿Desea aplicar estos cambios? [S/n]: ",
            Lang::Fr => "Voulez-vous appliquer ces modifications ? [O/n] : ",
            Lang::De => "Möchten Sie diese Änderungen anwenden? [J/n]: ",
            Lang::It => "Applicare queste modifiche? [S/n]: ",
            Lang::Ru => "Применить эти изменения? [Д/н]: ",
            Lang::Ja => "これらの変更を適用しますか? [Y/n]: ",
            Lang::Zh => "是否应用这些更改？ [Y/n]: ",
            Lang::En => "Apply these changes? [Y/n]: ",
        }
    }

    pub fn is_affirmative(&self, input: &str) -> bool {
        let s = input.trim().to_lowercase();
        if s.is_empty() {
            return true;
        }
        matches!(
            s.as_str(),
            "y" | "yes" | "s" | "sim" | "si" | "sí" | "o" | "oui" | "j" | "ja" | "д" | "да"
        )
    }

    pub fn abort_msg(&self) -> &'static str {
        match self {
            Lang::Pt => "Operação cancelada pelo usuário.",
            Lang::Es => "Operación cancelada por el usuario.",
            Lang::Fr => "Opération annulée par l'utilisateur.",
            Lang::De => "Vorgang vom Benutzer abgebrochen.",
            Lang::It => "Operazione annullata dall'utente.",
            Lang::Ru => "Операция отменена пользователем.",
            Lang::Ja => "ユーザーによって操作がキャンセルされました。",
            Lang::Zh => "操作已被用户取消。",
            Lang::En => "Operation cancelled by user.",
        }
    }

    pub fn root_required_err(&self) -> &'static str {
        match self {
            Lang::Pt => "Permissão negada em /var/lib/pkg/lock (você precisa executar com sudo/root).",
            Lang::Es => "Permiso denegado en /var/lib/pkg/lock (debe ejecutar como sudo/root).",
            Lang::Fr => "Permission refusée sur /var/lib/pkg/lock (exécutez avec sudo/root).",
            Lang::De => "Berechtigung verweigert für /var/lib/pkg/lock (bitte mit sudo/root ausführen).",
            Lang::It => "Permesso negato su /var/lib/pkg/lock (eseguire con sudo/root).",
            Lang::Ru => "Отказано в доступе к /var/lib/pkg/lock (запустите через sudo/root).",
            Lang::Ja => "/var/lib/pkg/lock へのアクセスが拒否されました (sudo/root で実行してください)。",
            Lang::Zh => "/var/lib/pkg/lock 权限被拒绝（请使用 sudo/root 运行）。",
            Lang::En => "Permission denied on /var/lib/pkg/lock (please run as sudo/root).",
        }
    }

    pub fn already_newest(&self, name: &str, version: &str) -> String {
        match self {
            Lang::Pt => format!("Pacote '{name}' ({version}) já está instalado e atualizado."),
            Lang::Es => format!("El paquete '{name}' ({version}) ya está instalado y actualizado."),
            Lang::Fr => format!("Le paquet '{name}' ({version}) est déjà installé et à jour."),
            Lang::De => format!("Paket '{name}' ({version}) ist bereits installiert und aktuell."),
            Lang::It => format!("Il pacchetto '{name}' ({version}) è già installato e aggiornato."),
            Lang::Ru => format!("Пакет '{name}' ({version}) уже установлен и обновлён."),
            Lang::Ja => format!("パッケージ '{name}' ({version}) は既に最新です。"),
            Lang::Zh => format!("软件包 '{name}' ({version}) 已安装且为最新版本。"),
            Lang::En => format!("Package '{name}' ({version}) is already installed and up-to-date."),
        }
    }

    pub fn pkg_not_installed(&self, name: &str) -> String {
        match self {
            Lang::Pt => format!("Pacote '{name}' não está instalado."),
            Lang::Es => format!("El paquete '{name}' no está instalado."),
            Lang::Fr => format!("Le paquet '{name}' n'est pas installé."),
            Lang::De => format!("Paket '{name}' ist nicht installiert."),
            Lang::It => format!("Il pacchetto '{name}' non è installato."),
            Lang::Ru => format!("Пакет '{name}' не установлен."),
            Lang::Ja => format!("パッケージ '{name}' はインストールされていません。"),
            Lang::Zh => format!("软件包 '{name}' 未安装。"),
            Lang::En => format!("Package '{name}' is not installed."),
        }
    }

    pub fn unable_to_locate(&self, name: &str) -> String {
        match self {
            Lang::Pt => format!("Pacote '{name}' não encontrado em nenhum repositório sincronizado."),
            Lang::Es => format!("Paquete '{name}' no encontrado en ningún repositorio."),
            Lang::Fr => format!("Paquet '{name}' introuvable dans les dépôts."),
            Lang::De => format!("Paket '{name}' wurde in keinem Repository gefunden."),
            Lang::It => format!("Pacchetto '{name}' non trovato nei repository."),
            Lang::Ru => format!("Пакет '{name}' не найден в репозиториях."),
            Lang::Ja => format!("パッケージ '{name}' がリポジトリに見つかりません。"),
            Lang::Zh => format!("在仓库中找不到软件包 '{name}'。"),
            Lang::En => format!("Package '{name}' not found in any synced repository."),
        }
    }
}
