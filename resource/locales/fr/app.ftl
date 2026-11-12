# App Fair — chaînes d'interface en français. Voir locales/en/app.ftl pour les clés de référence.

app_title = App Fair

# Onglets + titres de navigation
tab_catalogs = Catalogues
tab_updates = Mises à jour
tab_settings = Réglages
nav_updates = Mises à jour
nav_add_catalog = Ajouter un catalogue

# Boutons communs
btn_ok = OK
btn_cancel = Annuler
btn_close = Fermer
btn_done = Terminé

# Onglet Catalogues
btn_add = Ajouter
btn_refresh = Actualiser
search_placeholder = Rechercher des applis
all_catalogs = Tous les catalogues
all_categories = Toutes les catégories
catalog_option = { $name } ({ $count })

# Menu déroulant de tri
sort_default = Par défaut
sort_last_updated = Dernière mise à jour
sort_name = Nom

# Ligne d'état de synchronisation
sync_checking = Recherche de mises à jour…
sync_downloading_pct = Téléchargement du catalogue… { $pct } %
sync_downloading = Téléchargement du catalogue…
sync_building = Mise à jour du catalogue…
sync_up_to_date = Catalogue à jour
sync_updated = { $count ->
    [one] { $count } appli mise à jour
   *[other] { $count } applis mises à jour
}
sync_failed = Échec de la synchronisation : { $error }

# Onglet Mises à jour
updates_none = Aucune appli installée provenant de vos catalogues.
updates_up_to_date = { $count } installées · toutes à jour
updates_available = { $count ->
    [one] { $count } mise à jour disponible
   *[other] { $count } mises à jour disponibles
}
btn_update_all = Tout mettre à jour
status_up_to_date = À jour
status_update = Mettre à jour → { $version }

# Page de détail
btn_install = Installer
btn_update = Mettre à jour
btn_launch = Ouvrir
btn_uninstall = Désinstaller
btn_open = Ouvrir
btn_delete = Supprimer
app_not_found = Appli introuvable
unknown_author = Auteur inconnu
version_line = Version { $version }
version_line_size = Version { $version } · { $size }
compatible_yes = Compatible avec votre appareil
compatible_needs = Nécessite Android { $release } — plus récent que cet appareil
incompatible_title = { $name } n'est pas compatible
incompatible_body = Cette appli nécessite Android { $need }. Votre appareil utilise Android { $have }, elle ne peut donc pas être installée.
compatible_needs_ios = Nécessite iOS { $release } ou un matériel absent de cet appareil
incompatible_body_ios = Cette appli nécessite iOS { $need } ou une version ultérieure, ou un matériel que cet appareil n'a pas, elle ne peut donc pas être installée ici.
market_region = Apple n'autorise pas les boutiques d'applis alternatives dans votre région : App Fair ne peut donc pas installer d'applis sur cet appareil.
section_screenshots = Captures d'écran
section_whats_new = Nouveautés
section_permissions = Autorisations
section_antifeatures = Anti-fonctionnalités
license_line = Licence : { $license }
website_line = Site web : { $url }
source_line = Source : { $url }
package_line = Paquet : { $pkg }
signed_by = Signé par { $fp }
installed_from = Installé depuis { $source }
installed_by_app_fair = Installé par App Fair
launch_confirm = Ouvrir { $name } ?
launch_fail_title = Impossible d'ouvrir { $name }
uninstall_confirm = Désinstaller { $name } ?
uninstall_fail_title = Impossible de désinstaller { $name }
perm_system_label = Autorisation système :
antifeature_no_desc = Ce catalogue n'a pas fourni de description pour cette anti-fonctionnalité.

# Superposition d'installation
install_downloading = Téléchargement…
install_bytes = { $done } / { $total }
install_installing = Installation…
install_confirm_hint = Confirmez l'installation lorsque Android le demande.
install_authorizing = Autorisation de l'installation…
install_confirm_hint_ios = Confirmez l'installation quand iOS le demande.
install_done = Installé
install_cancelled = Annulé
install_failed = Échec de l'installation

# Onglet Réglages
settings_catalogs = Catalogues
settings_catalogs_blurb = Activez, désactivez ou supprimez vos catalogues.
settings_updates = Mises à jour
pref_sync_title = Vérifier les mises à jour au lancement
pref_sync_blurb = Synchroniser les catalogues à chaque ouverture d'App Fair.
pref_auto_title = Télécharger les mises à jour automatiquement
pref_auto_blurb = Récupère les mises à jour en arrière-plan ; chaque installation demande toujours confirmation.
clear_cache_title = Vider le cache d'images
clear_cache_blurb = Supprimer les icônes et captures d'écran en cache.
clear_cache_done = Vidé.
btn_remove = Supprimer
settings_hide = Masquer les applis avec…
settings_storage = Stockage
settings_hide_blurb = Exclure de la navigation et de la recherche les applis déclarant ces anti-fonctionnalités.
about_blurb = Une boutique d'applis ouverte pour les catalogues App Index, AltStore et F-Droid, conçue avec Day.
about_privacy = App Fair ne communique qu'avec les catalogues que vous ajoutez. Aucun compte, aucune analyse — les vérifications de mises à jour et les téléchargements vont directement à ces dépôts, et rien de ce que vous consultez ou installez ne quitte votre appareil.
about_privacy_ios = App Fair ne communique qu'avec les catalogues que vous ajoutez et, quand vous installez ou mettez à jour une appli, avec le serveur de la boutique App Fair, qui autorise chaque installation auprès d'iOS. Ce serveur ne voit qu'un identifiant anonyme créé sur cet appareil, jamais votre nom ni votre compte Apple. Aucun compte, aucune analyse.
repo_row_meta = { $address }  ·  { $count } applis

# Libellés du filtre d'anti-fonctionnalités
af_ads = Publicité
af_tracking = Vous piste
af_nonfreenet = Services réseau non libres
af_nonfreeadd = Modules non libres
af_nonfreedep = Dépendances non libres
af_upstreamnonfree = Amont non libre
af_nonfreeassets = Ressources non libres
af_knownvuln = Vulnérabilité connue
af_nosourcesince = Sans source depuis

# Page d'ajout de catalogue
add_title = Ajouter un catalogue
add_blurb = Saisissez l'adresse d'un App Index (appindex.json), d'une source AltStore ou d'un dépôt F-Droid (avec son ?fingerprint=), ou choisissez un dépôt connu.
add_placeholder = https://exemple.org/appindex.json
add_error_insecure = Les URL de catalogue doivent utiliser https:// (ou une adresse .onion).
btn_add_catalog = Ajouter le catalogue
add_known = Dépôts connus
