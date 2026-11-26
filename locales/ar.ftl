# Ibis’s words, for the command line, agents (MCP) and the app (#1675).
# The engine returns codes; each code has one sentence here, which every front
# end shows. A `.cli`, `.agent` or `.app` attribute says what to do next in that
# front end. Ids come from the codes’ wire names, and a test fails on a code
# without words or words without a code. People’s words, not the engine’s
# (design principle 12), and typographic apostrophes.

-library-formats = BibTeX أو BibLaTeX أو CSL-JSON أو RIS أو CSV

## Engine errors: what went wrong, by code.

error-usage = الأمر غير صالح.
    .cli = راجع: ibis --help
    .agent = الوسائط لا تطابق مخطط إدخال هذه الأداة؛ تحقق من الأسماء والأنواع والحدود والتركيبات التي تسمح بها.
error-bad-input = الإدخال غير صالح.
    .cli = راجع: ibis --help
    .agent = الوسائط لا تطابق مخطط إدخال هذه الأداة؛ تحقق من الأسماء والأنواع والحدود والتركيبات التي تسمح بها.
error-not-found = لا توجد نتائج مطابقة.
    .agent = استخدم ref أعادته أداة search.
error-source-unavailable = تعذّرت قراءة إحدى المكتبات الآن.
    .cli = حاول مرة أخرى.
    .agent = حاول مرة أخرى بعد قليل.
    .app = حاول مرة أخرى.
error-resource-limit = الطلب أو نتيجته كبير جدًا.
    .cli = اطلب أقل.
    .agent = اطلب نتائج أقل.
error-newer-format = كتب إصدار أحدث من Ibis هذه البيانات. حدّث Ibis.
error-internal = تعذّر على Ibis إكمال هذا.
    .cli = يُرجى الإبلاغ عنه على https://github.com/ibiscite/ibis/issues
    .agent = حاول مرة أخرى.
    .app = حاول مرة أخرى.
error-cancelled = أُلغي الطلب.

## Why, when the code alone cannot say.

reason-unknown-style = لا يوجد نمط بهذا الاسم.
    .cli = اعرض الأنماط باستخدام: ibis styles
    .agent = تعرضها أداة styles.
reason-unknown-output = لا يوجد إخراج بهذا المعرّف.
    .cli = اعرضها باستخدام: ibis outputs
    .agent = استخدم تنسيقًا من مخطط إدخال هذه الأداة.
reason-unknown-locale = لغة الاستشهاد هذه غير متاحة.
    .cli = اعرضها باستخدام: ibis styles --locales
    .agent = تعرض أداة styles اللغات المتاحة.
reason-duplicate-key = تعطي المكتبة مفتاح الاستشهاد هذا لأكثر من مُدخل، لذا يتخطاها Ibis كلها. أعطِ كل مُدخل مفتاحًا خاصًا به.
reason-damaged-entry = في المكتبة مُدخل بمفتاح الاستشهاد هذا، لكن تعذّر على Ibis قراءته، لذا يتخطاه.
    .cli = أصلح المُدخل؛ يبيّن ibis libraries السطر والسبب.
    .agent = أخبر الشخص أن المُدخل الذي يحمل هذا المفتاح تالف ويجب إصلاحه في مكتبته.
    .app = أصلح المُدخل في مكتبتك.
reason-ambiguous-key = عدة مراجع تستخدم مفتاح الاستشهاد هذا.
    .cli = أعطِ كلًّا منها مفتاحًا خاصًا به في مكتبتك.
    .agent = استخدم ref الذي أعادته أداة search.
    .app = أعطِ كلًّا منها مفتاحًا خاصًا به في مكتبتك.
reason-missing-key = ليس لهذا المرجع مفتاح استشهاد.
    .cli = أعطه مفتاحًا في مكتبتك، أو استخدم --style.
    .agent = استخدم نمطًا، أو اطلب من الشخص أن يعطيه مفتاحًا.
    .app = اختر نمط استشهاد.
reason-missing-file = نُقل ملف المكتبة أو أُعيدت تسميته أو أُزيل.
    .cli = اختر موقعه الحالي باستخدام --file.
    .agent = اطلب من المستخدم تحديد موقع ملف المكتبة، ثم ابدأ جلسة جديدة.
    .app = حدّد موقع ملف المكتبة.
reason-app-missing = لم يُعثر على تطبيق Ibis.
    .cli = ثبّته، أو حدّد موقعه باستخدام --app <path>.
reason-not-ibis = البرنامج المحدد على أنه تطبيق Ibis ليس Ibis.
    .cli = حدّد موقع تطبيق Ibis باستخدام --app <path>.
reason-no-answer = بدأ تطبيق Ibis لكنه لم يستجب.
    .cli = افتح Ibis يدويًا مرة واحدة، ثم حاول مرة أخرى.
reason-settings-read-timeout = انتهت مهلة قراءة إعدادات المكتبة في تطبيق Ibis.
reason-zotero-sharing-off = Zotero مفتوح لكنه لا يشارك مكتبتك مع التطبيقات الأخرى.
    .cli = في Zotero، افتح الإعدادات ثم «متقدم»، وفعّل «السماح للتطبيقات الأخرى على هذا الكمبيوتر بالتواصل مع Zotero».
    .agent = اطلب من الشخص تفعيل «السماح للتطبيقات الأخرى» في إعدادات Zotero المتقدمة.
reason-zotero-profile-needed = Zotero مغلق، ويحتاج Ibis إلى معرفة أي ملف تعريف من ملفاته يقرأ.
    .cli = افتح Zotero، أو سمِّ ملف تعريف باستخدام --zotero-profile NAME.
    .agent = اطلب من الشخص فتح Zotero أو اختيار ملف تعريف في Ibis أو تمرير --zotero-profile إلى ibis mcp.
    .app = افتح Zotero، أو اختر ملف تعريف.
reason-zotero-needs-opening = لا يمكن قراءة مكتبة Zotero الخاصة بك وZotero مغلق.
    .cli = افتح Zotero، ثم حاول مرة أخرى.
    .agent = اطلب من الشخص فتح Zotero.
reason-zotero-not-found = لم يُعثر على مكتبة Zotero على هذا الكمبيوتر.
    .cli = تأكد من أن Zotero مثبّت وفُتح مرة واحدة على الأقل، أو استخدم --file.
reason-zotero-no-space = لا توجد مساحة فارغة كافية لقراءة Zotero وهو مغلق.
    .cli = أفرغ بعض المساحة، أو افتح Zotero.
    .app = أفرغ بعض المساحة، أو افتح Zotero.
reason-csv-template = الصف الأول من ملف CSV هذا لا يطابق قالب Ibis: يجب أن يسمّي عمودًا للنوع وآخر للعنوان، ولا أعمدة غير التي يذكرها القالب.
    .cli = أعد تسمية الأعمدة في صفه الأول؛ يذكرها قالب CSV في دليل التوافق الخاص بـ Ibis.
    .agent = اطلب من الشخص إعادة تسمية الأعمدة في الصف الأول من الملف بأسماء القالب.
    .app = أعد تسمية الأعمدة في صفه الأول؛ يذكرها قالب CSV في دليل التوافق الخاص بـ Ibis.
reason-library-cut-short = لا يحمل أي مرجع هذا المفتاح فيما قرأه Ibis، لكن المكتبة أكبر من أن تُقرأ كاملة (يتوقف Ibis عند 200,000 مُدخل)، لذا قد يكون المفتاح في الجزء الذي لم يقرأه.
    .cli = قسّم المكتبة إلى ملفات أصغر وحدّد كلًّا منها باستخدام --file.
    .agent = اطلب من الشخص تقسيم المكتبة المصدر إلى ملفات أصغر ليتمكن Ibis من قراءة المراجع الناقصة.
    .app = قسّم المكتبة إلى ملفات أصغر.
reason-unknown-format = هذا الملف ليس بتنسيق يقرؤه Ibis: { -library-formats }.
    .cli = اختر ملفًا آخر باستخدام --file.
    .agent = اطلب من الشخص اختيار ملف آخر.
    .app = اختر ملفًا آخر.
reason-duplicate-record = سجلّان مُرسلان مع الطلب لهما المعرّف نفسه (يُعدّ 1 و"1" معرّفًا واحدًا)، لذا لا يستطيع Ibis معرفة المقصود.
    .cli = أعطِ كل سجل معرّفًا خاصًا به.
    .agent = أعطِ كل سجل معرّفًا خاصًا به.
reason-rejected-record = أحد السجلات المُرسلة مع الطلب ليس CSL-JSON صالحًا، لذا لا يستشهد Ibis بأي منها بدلًا من استبعاده.
    .cli = يحتاج كل سجل إلى id ونوع CSL؛ أصلحه أو أزله.
    .agent = يحتاج كل سجل إلى id ونوع CSL مثل book أو article-journal؛ أصلح السجل غير الصالح أو أزله.
reason-no-attachment-text = لا يستطيع Ibis قراءة نص هذا المرفق: إما أن Zotero لم يفهرسه، أو أنه رابط، أو أن ملفه مفقود أو تالف أو لا يستطيع Ibis قراءته.
    .cli = شغّل ibis outline على مرجعه لمعرفة السبب؛ في Zotero يضيف «إعادة فهرسة العنصر» النص.
    .agent = أخبر المستخدم؛ يمكنه إضافة النص في Zotero عبر «إعادة فهرسة العنصر». لا تخمّن ما يقوله.
reason-attachment-outside-library = ملف هذا المرفق خارج مجلد المكتبة. يمكنك السماح به من الإعدادات › تطبيقات الذكاء الاصطناعي › نص المرفقات.
reason-no-such-pages = ليس في هذا المرفق نص لتلك الصفحات، أو أن بداية صفحاته غير معروفة.
    .cli = شغّل ibis outline على مرجعه لمعرفة الصفحات التي فيها نص، أو اقرأ دون --pages.
    .agent = استخدم الصفحات التي يعطيها outline، أو اقرأ باستخدام offset إذا كانت صفحاته غير معروفة.
reason-destination-exists = يوجد هناك شيء بهذا الاسم. اختر اسمًا أو مكانًا آخر؛ لا يستبدله Ibis أبدًا.
reason-destination-unwritable = تعذّر على Ibis الكتابة هناك. اختر مجلدًا موجودًا يمكنك الكتابة فيه، وعلى قرصه مساحة متبقية.
reason-source-not-found = تعذّر على Ibis تشغيل محوّل مصدر بهذا الاسم.
    .cli = تحقّق من الاسم بعد --source ومن المسار المطلق للملف التنفيذي في source-adapters.json. شغّل ibis doctor لعرض المحوّلات المُعدّة.
    .agent = اطلب من المستخدم التحقق من اسم المصدر وإعداده في ibis doctor.
reason-source-manager-closed = مدير المراجع غير مفتوح، وهذا المصدر لا يقرؤه إلا وهو مفتوح.
    .cli = افتح مدير المراجع ومكتبته، ثم حاول مرة أخرى.
    .agent = اطلب من المستخدم فتح مدير المراجع ومكتبته.
reason-source-library-needed = عدة مكتبات مفتوحة في مدير المراجع، لذا يحتاج Ibis إلى معرفة أيّها يقرأ.
    .cli = سمِّ واحدة بعد المصدر، مثل --source endnote=NAME.
    .agent = اسأل المستخدم أيّ مكتبة يجب قراءتها.
reason-source-library-not-found = لا تحمل أيٌّ من مكتبات مدير المراجع المفتوحة هذا الاسم.
    .cli = افتح المكتبة في مدير المراجع، أو تحقّق من اسمها بعد --source.
    .agent = اطلب من المستخدم فتح المكتبة في مدير المراجع.
reason-source-permission = لم يسمح نظام التشغيل لـ Ibis بأن يطلب من مدير المراجع مكتبته.
    .cli = في macOS، اسمح بذلك في إعدادات النظام، الخصوصية والأمن، الأتمتة، ثم حاول مرة أخرى.
    .agent = اطلب من المستخدم أن يسمح لـ Ibis بالتحكم في مدير المراجع من إعدادات الخصوصية في نظامه.
reason-source-failed = لم يقرأ محوّل المصدر المكتبة: إما أنه لم يستطع، أو توقف، أو استغرق وقتًا طويلًا، أو أجاب بطريقة لا يقبلها Ibis.
    .cli = حاول مرة أخرى؛ يعرض ibis sources check ما يجيب به المحوّل.
    .agent = اطلب من المستخدم تشغيل ibis sources check لهذا المصدر.
app-share-page-title = قائمة القراءة
app-share-page-import = يحمّل References.bib هذه المراجع إلى Zotero أو EndNote أو أي مدير مراجع آخر.
app-share-page-pdfs = يضم مجلد PDFs ملفات PDF التي جاءت معها.

## What a citation could not say in full.

warning-source-field = تعذّر تحويل أحد حقول { $reference } بالكامل. تحقق منه في مكتبتك.
warning-sentence-case-expected = يتوقع APA أسلوب الجملة في عنوان { $reference }. أُبقيت الأحرف الكبيرة كما هي مخزّنة، فتحقق منها في مستندك.
loss-locator = الصفحة أو الموضع
loss-prefix = النص السابق
loss-suffix = النص اللاحق
loss-author-suppression = المؤلف المحذوف
loss-rich-text = تنسيق النص
loss-order = ترتيب المراجع
notice-needs-document-context = يرقّم هذا النمط الاستشهادات أو يضع الحواشي على امتداد المستند كله، وهو ما لا يراه Ibis. تحقق من النتيجة في مستندك.
notice-reference-entry = هذا هو مُدخل المرجع الكامل.
notice-full-first-note = هذه هي الصيغة الكاملة التي تستخدمها الحاشية الأولى؛ الحواشي اللاحقة للعمل نفسه عادةً أقصر.
notice-year-suffix-withheld = عملان من هذه الأعمال لهما المؤلف والسنة نفسها. أضف أحرف السنة (مثل 2026a و2026b) في مستندك، لأنها تعتمد على قائمة المراجع كاملة.
notice-input-language-unsupported = لا يتعرّف Ibis على كلمات الموضع بهذه اللغة. اكتب الصفحة أو الموضع بالإنجليزية، مثل «p. 12».

## Citation forms, by their one name.

form-normal = الاسم بين قوسين
form-narrative = الاسم داخل الجملة
form-author-only = الاسم فقط
form-year-only = السنة فقط
form-full = المرجع كاملًا
form-bibliography-only = قائمة المراجع

## Why a library skipped a record or asks for a field to be checked. `.field`
## is used when the source names the field.

cause-not-closed = لا يُغلَق أبدًا
cause-no-key = ليس له مفتاح استشهاد
cause-missing-value = أحد الحقول بلا قيمة
    .field = { $field } بلا قيمة
cause-unbalanced = أقواس أحد الحقول أو علامات اقتباسه غير متطابقة
    .field = الأقواس أو علامات الاقتباس في { $field } غير متطابقة
cause-unexpected-text = فيه نص حيث يجب أن يكون حقل
    .field = في { $field } نص لا يستطيع Ibis قراءته
cause-unknown-type = نوعه غير معروف لـ Ibis
cause-conflicting = فيه قيم حقول مكررة أو متعارضة
    .field = { $field } مذكور أكثر من مرة
cause-wrong-cells = عدد خلاياه أكثر أو أقل من الصف الأول
cause-missing-field = أحد الحقول المطلوبة فارغ
    .field = { $field } فارغ
cause-no-start = حقوله تأتي قبل أي سطر TY
cause-string-definition = تعذّرت قراءة تعريف @string
cause-unreadable = تعذّرت قراءته
cause-abbreviation = أحد الحقول يستخدم اختصارًا غير معرَّف أو لا يمكن توسيعه
    .field = { $field } يستخدم اختصارًا غير معرَّف أو لا يمكن توسيعه
cause-date = تعذّرت قراءة تاريخ بالكامل
    .field = تعذّرت قراءة { $field } بالكامل كتاريخ
cause-names = تعذّرت قراءة قائمة أسماء
    .field = تعذّرت قراءة الأسماء في { $field }
cause-role = أحد الأدوار ليس له مقابل في CSL، لذا تُعدّ تلك الأسماء مساهمين
    .field = ليس لـ { $field } مقابل في CSL، لذا تُعدّ تلك الأسماء مساهمين
cause-link = أحد الروابط خاص أو ليس عنوان ويب، لذا استُبعد
    .field = { $field } خاص أو ليس عنوان ويب، لذا استُبعد
cause-inheritance = إحالة مرجعية مفقودة أو تدور في حلقة
    .field = يشير { $field } إلى مُدخل مفقود أو يدور في حلقة
cause-inheritance-limit = بلغ Ibis حدّه في قراءة المُدخلات المرتبطة
    .field = يبلغ { $field } حدّ Ibis في قراءة المُدخلات المرتبطة
cause-too-large = أحد الحقول أكبر من أن يُقرأ
    .field = { $field } أكبر من أن يُقرأ
cause-identifier = أحد المعرّفات غير صالح
    .field = { $field } ليس معرّفًا صالحًا
cause-language = لغته ليست لغة يتعرّف عليها Ibis
    .field = يذكر { $field } لغة لا يتعرّف عليها Ibis
cause-tex-command = يستخدم أمر TeX لا يفهمه Ibis، فأُبقي كما هو مكتوب
cause-unknown-field = أحد الحقول ليس من حقول التنسيق، لذا استُبعد
    .field = { $field } ليس من حقول التنسيق، لذا استُبعد
cause-value = تعذّرت قراءة قيمة كما هي مكتوبة
    .field = تعذّرت قراءة { $field } كما هو مكتوب

## The command line’s own sentences about a refusal.

cli-together-refused = لا يستطيع { $output } كتابة هذه المراجع معًا كما طُلب. استشهد بها واحدًا تلو الآخر لمعرفة أيها يفشل.
cli-loss-left-out = لا يستطيع { $output } عرض { $what }{ $target }، لذا استُبعد.
cli-loss-flattened = كتب { $output } { $what }{ $target } نصًا عاديًا.
cli-writing-without-bibliography = لا يوجد ملف ‎.bib
cli-loss-unknown = غيّر { $output } { $what }{ $target } بطريقة لا يستطيع هذا الإصدار من Ibis تسميتها. تحقق من الاستشهاد.
cli-loss-something = جزء من الاستشهاد
cli-doctor-about = افحص مكتباتك وإعداداتك دون اتصال، دون كتابة أو إرسال أي شيء
cli-doctor-summary = Ibis { $version } · العقد { $contract } · طرق المحرك: { $methods }، تنسيقات المفاتيح: { $outputs }، أنماط الاستشهاد: { $styles }
cli-spelling-match = (تطابق إملائي)
cli-no-references = لا تحتوي أي مكتبة مفتوحة على مراجع. أضف إليها مراجع، أو افتح مكتبة أخرى باستخدام --file أو --zotero. يجب أن يكون الملف بتنسيق { -library-formats }. لملفات CSV، استخدم قالب Ibis.
cli-file-help = افتح ملفًا بتنسيق { -library-formats } أو مجلد PDF حفظه Ibis؛ كرّر لعدة مكتبات
cli-folder-kept = { $folder }: حُفظ { $kept } من ملفات PDF في ibis-references.bib ({ $unread } بلا تفاصيل استطاع Ibis قراءتها)؛ { $retry } لإعادة المحاولة لاحقًا
cli-folder-not-kept = لم يحفظ Ibis ملفات PDF في هذا المجلد بعد؛ شغّل أولًا ibis sources keep --file FOLDER
cli-file-unreadable = تتعذّر قراءته، أو ليس مكتبة بتنسيق { -library-formats }؛ تحقق من أذوناته ومحتواه.
app-file-dialog-title = اختر مكتبة
app-file-dialog-formats = { -library-formats }

## The app’s native windows and dialogs, and its words about other apps.

app-window-settings = إعدادات Ibis
app-window-notice = إشعار Ibis
app-open-link-title = فتح الرابط
app-open-link-copied = تعذّر على Ibis فتح متصفحك. نُسخ الرابط: الصقه في متصفحك.
app-open-link-failed = تعذّر على Ibis فتح متصفحك. افتح هذا الرابط في متصفحك:
app-paste-list-before-27 = تسهيلات الاستخدام
app-paste-list-since-27 = التحكم في الجهاز والوصول إلى البيانات
app-agent-steps-chatgpt = في ChatGPT لسطح المكتب: الإعدادات › خوادم MCP › إضافة خادم › STDIO، الأمر ibis، الوسائط mcp --app-libraries.
app-proposal-add = إضافة «{ $title }» باسم { $key }.
app-proposal-fields = تغيير { $fields } في «{ $label }».
app-proposal-tags =
    { $state ->
        [add] إضافة الوسوم { $add } إلى «{ $label }».
        [remove] إزالة الوسوم { $remove } من «{ $label }».
       *[both] إضافة الوسوم { $add } إلى «{ $label }» وإزالة الوسوم { $remove } منه.
    }

app-update-question-title = التحقق من التحديثات تلقائيًا؟
app-update-question = يستطيع Ibis التحقق من GitHub بحثًا عن إصدار أحدث بضع مرات في اليوم. يطلب من GitHub ملفًا صغيرًا واحدًا؛ ويرى GitHub عنوان IP الخاص بك وإصدار Ibis. لا يُرسل شيء من مكتبتك، ولا تُثبَّت التحديثات إلا بموافقتك.
app-update-check-automatically = التحقق تلقائيًا
app-update-not-now = ليس الآن
app-update-offer-title = يتوفر تحديث
app-update-offer = يتوفر Ibis { $version }؛ لديك { $current }.
app-update-offer-steps = سينزّله Ibis ويتحقق من توقيعه ويثبّته ثم يعيد التشغيل.
app-update-offer-question = تثبيت هذا التحديث وإعادة تشغيل Ibis؟
app-update-later = لاحقًا
app-update-install = التثبيت وإعادة التشغيل
app-update-cancel = إلغاء
app-update-install-failed = تعذّر تثبيت التحديث. حاول مرة أخرى لاحقًا. إن لم يعد Ibis يفتح، فنزّل أحدث إصدار من GitHub وأعد تثبيته.
app-updates-title = التحديثات

## The app’s own sentence about a citation.

app-loss = لا يستطيع هذا الإخراج عرض { $what } بالكامل.
    .named = لا يستطيع هذا الإخراج عرض { $what } لـ { $reference } بالكامل.
app-project-warning =
    { $state ->
        [missing] { $key } غير موجود في { $name }.
        [repeated] { $key } مذكور أكثر من مرة في { $name }.
        [may-name-another] قد يشير { $key } إلى مرجع آخر في { $name }.
       *[could-not-check] تعذّر التحقق من { $key } في { $name }.
    }
app-tray-open = فتح Ibis
app-tray-open-shortcut = فتح Ibis ({ $shortcut })
app-key-control = Control
app-key-option = Option
app-key-shift = Shift
app-key-command = Command
app-tray-choose = اختيار ملف…
app-tray-zotero = Zotero
app-tray-retry = إعادة المحاولة
app-tray-settings = الإعدادات…
app-tray-update = التحقق من وجود تحديثات…
app-tray-help = مساعدة
app-tray-quit = إنهاء { $name }
app-tray-shortcuts = اختصارات لوحة المفاتيح
app-tray-whats-new = ما الجديد
app-tray-problem = الإبلاغ عن مشكلة…
app-tray-idea = اقتراح فكرة…
app-tray-reset = إعادة تعيين موضع النافذة
app-tray-fix-library = إصلاح في Ibis…
app-tray-library = المكتبة
app-tray-no-library = لا توجد مكتبة بعد
app-tray-opening = جارٍ فتح { $name }…
app-tray-reading = جارٍ قراءة { $name }…
app-tray-count = { $name } · المراجع: { $count }
app-tray-count-one = { $name } · مرجع واحد
app-tray-zotero-newer = يحتاج Zotero إلى إصدار أحدث من Ibis
app-tray-file-missing = نُقل { $name } أو أنه غير متاح
app-tray-file-unreadable = تعذّرت قراءة { $name }
app-tray-file-format = { $name } ليس ملف مكتبة مدعومًا
app-tray-file-csv = يحتاج { $name } إلى قالب CSV الخاص بـ Ibis
app-tray-file-too-large = حجم { $name } أكبر من 64 MiB
app-tray-file-invalid = لا يحتوي { $name } على مراجع صالحة
app-tray-zotero-sharing = Zotero لا يشارك مكتبتك
app-tray-zotero-profile = اختر ملف تعريف Zotero الذي يُقرأ
app-tray-zotero-missing = لم يُعثر على مكتبة Zotero
app-tray-zotero-closed = افتح Zotero لقراءة مكتبتك
app-tray-zotero-no-space = أفرغ بعض المساحة أو افتح Zotero لقراءة مكتبتك
app-tray-zotero-group-missing = لم يُعثر على مكتبة مجموعة مختارة في Zotero
app-tray-zotero-work-limit = استغرقت قراءة Zotero وقتًا طويلًا
app-tray-not-updated = تعذّر تحديث { $name }
app-tray-not-opened = تعذّر فتح { $name }
app-tray-version-copy-failed = تعذّر نسخ الإصدار. حاول مرة أخرى.
app-tray-fix-problem = إصلاح مشكلة واحدة…
app-tray-fix-problems = إصلاح المشكلات ({ $count })…
app-tray-fix-failed = تعذّر فتح المكتبة للتحرير. افتحها من نافذة Ibis لتحاول مرة أخرى.
app-tray-spoken-reading = Ibis، جارٍ قراءة { $name }
app-tray-spoken-problem = Ibis، { $problem }
app-tray-spoken-empty = Ibis، لا توجد مكتبة بعد
app-tray-spoken-ready = Ibis، جاهز
app-saved-title = حفظ المراجع المستشهد بها منذ الفتح
app-saved-file = مستشهد بها منذ الفتح
app-saved-page = المراجع المستشهد بها منذ الفتح

## The app’s own words when a library cannot be read or a command fails.
# Most are joined into a longer line, so they start in lower case.

app-library-file-unavailable = ملف المكتبة غير متاح؛ نزّل أولًا أي ملف متاح عبر الإنترنت فقط إلى هذا الكمبيوتر
app-library-file-invalid = لا يحتوي ملف المكتبة على بيانات مراجع صالحة
app-library-file-too-large = حجم ملف المكتبة أكبر من 64 MiB
app-library-file-unfinished = يحتوي ملف المكتبة على مُدخل غير مكتمل
app-library-work-limit = تجاوزت المكتبة حدّ العمل المسموح به للتحميل
app-library-file-not-utf8 = ملف المكتبة ليس نصًا بترميز UTF-8؛ احفظه بترميز UTF-8
app-library-zotero-sharing-off = Zotero مفتوح لكنه لا يشارك مكتبتك مع التطبيقات الأخرى
app-library-zotero-profile-needed = اختر ملف تعريف Zotero الذي يُقرأ
app-library-zotero-closed = لا يمكن قراءة مكتبة Zotero وهو مغلق
app-library-zotero-not-found = لم يُعثر على مكتبة Zotero على هذا الكمبيوتر
app-library-zotero-no-space = لا توجد مساحة فارغة كافية لقراءة Zotero وهو مغلق؛ أفرغ بعض المساحة أو افتح Zotero
app-library-zotero-cancelled = أُلغيت قراءة Zotero
app-library-zotero-work-limit = تجاوزت قراءة Zotero حدّ العمل المسموح به
app-library-zotero-newer = قاعدة بيانات Zotero هذه أحدث مما يستطيع Ibis قراءته
app-library-zotero-group-not-found = لم يُعثر على مكتبة مجموعة مختارة في Zotero؛ تأكد من أنك لا تزال عضوًا فيها
app-library-zotero-try-again = تعذّرت قراءة Zotero الآن؛ حاول مرة أخرى
app-library-not-local = الملف المختار ليس على هذا الكمبيوتر
app-library-stopped = توقف تحميل المكتبة على نحو غير متوقع
# $error is one of the sentences above.
app-library-kept = { $error }؛ يُحتفظ بآخر المراجع المحمّلة
# $name is the library’s name; $error is one of the sentences above.
app-library-retained-not-updated = تعذّر تحديث المكتبة المحتفظ بها { $name }: { $error }
app-library-not-refreshed = تعذّر تحديث المكتبة؛ يُحتفظ بآخر المراجع المحمّلة.
app-library-retained-not-refreshed = تعذّر تحديث المكتبة المحتفظ بها؛ يُحتفظ بآخر المراجع المحمّلة.
app-library-not-saved = فُتحت المكتبة لكن تعذّر حفظها للمرة القادمة: { $error }
app-command-snapshot-changed = تغيّرت المكتبة. ابحث مرة أخرى.
app-command-library-loading = جارٍ تحميل المكتبة
app-command-cancelled = أُلغي
app-command-shared-citation-key = مفتاح الاستشهاد `{ $key }` يخص أكثر من مرجع
app-command-citation-too-large = يحتوي هذا الاستشهاد على نص كثير جدًا. اختصر حقوله أو أزل بعض المراجع.
app-command-not-prepared = تعذّر تجهيز الاستشهاد. حاول مرة أخرى.
app-command-source-not-opened = تعذّر فتح المصدر. حاول مرة أخرى.
## The app’s interface.

# Citation language and text direction have separate owners.
# Settings interface copy; native guidance and key notation keep their owners.
settings-pages = صفحات الإعدادات
settings-general = عام
settings-shortcuts = الاختصارات
settings-about = حول Ibis
settings-about-description = الإصدار والتحديثات وأصل Ibis.

general-login-label =
    { $platform ->
        [macos] الفتح عند تسجيل الدخول
        [windows] التشغيل عند تسجيل الدخول
       *[linux] التشغيل عند تسجيل الدخول
    }
general-login-description = يُبقي الاختصار جاهزًا بعد إعادة التشغيل.
general-login-place =
    { $platform ->
        [macos] إعدادات النظام > عام > عناصر تسجيل الدخول
        [windows] الإعدادات > التطبيقات > بدء التشغيل
       *[linux] إعدادات تطبيقات بدء التشغيل في سطح المكتب لديك
    }
general-login-when =
    { $platform ->
        [windows] تسجيل الدخول
       *[other] تسجيل الدخول
    }
general-login-unsupported = لا يستطيع هذا النظام تشغيل { $name } عند { general-login-when }. يمكنك إضافته من { general-login-place }.
general-login-needs-approval = ينتظر macOS موافقتك. فعّل { $name } في { general-login-place }.
general-login-refused = لم يسمح النظام لـ { $name } بتغيير هذا. يمكنك تغييره من { general-login-place }.
general-login-unreadable = تعذّر على { $name } التحقق مما إذا كان يعمل عند { general-login-when }. راجع { general-login-place }.

general-menu-bar-label = الإظهار في شريط القوائم
general-language = اللغة
general-language-description = القوائم والإعدادات والرسائل، بدءًا من المرة القادمة التي يُفتح فيها Ibis. تحتفظ الاستشهادات بلغة نمطها.
general-language-system = مثل النظام
general-language-words = لغة الواجهة ترجمة الإنجليزية الألمانية العربية language
# Each language by its own name, the same in every catalogue.
language-english = English
language-german = Deutsch
language-arabic = العربية
general-placement = مكان فتح Ibis
general-placement-description = على الشاشة التي تعمل عليها، ما لم تكن قد نقلت Ibis إلى مكان آخر فيها.
general-placement-words = موضع مكان شاشة عرض زاوية وسط منتصف فتح
general-placement-centre = الوسط
general-placement-top-left = أعلى اليسار
general-placement-top-right = أعلى اليمين
general-placement-bottom-left = أسفل اليسار
general-placement-bottom-right = أسفل اليمين
general-menu-bar-description = إذا أخفيته، فافتح Ibis من مجلد التطبيقات للعودة إلى هنا.
general-paste-title = اللصق
general-behavior-title = السلوك
general-paste-label = اللصق في التطبيق الذي جئت منه
general-paste-key =
    { $platform ->
        [macos] ⌘V
       *[other] Ctrl+V
    }
general-paste-description = وإلا فسينسخ Ibis وتضغط أنت { general-paste-key }.
# $list is the system's permission-list name, not an interface message code.
general-paste-allowed = مسموح في الخصوصية والأمن › { $list }
general-paste-allowed-note = يضغط Ibis ⌘V مرة واحدة، وفقط في التطبيق الذي فتحته منه.
general-paste-guide-title = اسمح لـ Ibis باللصق نيابةً عنك
general-paste-guide-why = ليلصق بأمان، يحتاج Ibis إلى إذن تسهيلات الاستخدام ليتحقق من النافذة التي تتلقى الكتابة، وإلى إذن بضغط ⌘V. ولا يقرأ أبدًا ما تكتبه.
general-paste-step-open = انقر على «فتح إعدادات النظام».
general-paste-step-allow = في الخصوصية والأمن › { $list }، فعّل { $name }.
general-paste-step-return = عُد إلى مستندك وافتح Ibis مجددًا. اضغط Enter للمحاولة مرة أخرى.
general-paste-pane = الخصوصية والأمن › { $list }
general-paste-open = فتح إعدادات النظام
general-paste-drag = اسحب { $name } إلى { $list }
general-paste-drag-keyboard = اضغط لفتح إعدادات النظام.
general-paste-drag-fallback = إذا لم يكن Ibis في القائمة، فاسحبه إليها.
general-paste-drag-failed = تعذّر سحب نسخة Ibis هذه. استخدم الزر + في إعدادات النظام لإضافتها.
general-paste-waiting = بانتظار موافقتك…
general-paste-unreadable = تعذّر على Ibis قراءة هذا الإذن، لذا يبقى اللصق متوقفًا حتى يتمكن من ذلك.
general-paste-granted = يستطيع Ibis اللصق الآن.
general-paste-return-failed = تعذّرت العودة إلى استشهادك. افتح Ibis باختصارك للمتابعة.
general-paste-stale = مفعّل بالفعل، لكنه لا يزال ينتظر؟ في إعدادات النظام، أزل { $name } من القائمة، وأضف هذه النسخة مجددًا، ثم فعّلها.

general-rows = الصفوف
general-rows-description = الصفوف المضغوطة تعرض مراجع أكثر، سطرًا لكل مرجع؛ والمريحة تمنح كل مرجع سطرين.
general-density-compact = مضغوطة
general-density-comfortable = مريحة

shortcuts-open = فتح Ibis
shortcuts-description = انقر على الاختصار، ثم اكتب اختصارًا جديدًا. Esc للإلغاء.
shortcuts-record = اكتب اختصارًا…
shortcuts-use-default = استخدام الافتراضي
shortcuts-change = تغيير…
shortcuts-not-assigned = غير معيّن
shortcuts-configure-failed = تعذّر على سطح المكتب فتح إعدادات الاختصارات. حاول مرة أخرى.
shortcuts-failed = تعذّر على Ibis تغيير الاختصار. حاول مرة أخرى.
shortcuts-read-failed = تعذّر على Ibis التحقق من الاختصار. جرّب إعادة فتح الإعدادات.
shortcuts-refusal-invalid = لا يستطيع Ibis استخدام تركيبة المفاتيح هذه. جرّب اختصارًا آخر.
shortcuts-refusal-needs-modifier = أضف Control أو Alt أو Command. لا يكفي Shift وحده.
shortcuts-refusal-app-shortcut = هذا الاختصار مخصص لأوامر التطبيق. أضف مفتاح تعديل آخر.
shortcuts-refusal-alt-gr = يُستخدم Ctrl+Alt مع مفتاح كتابة بديلًا عن AltGr في Windows. جرّب اختصارًا آخر.
shortcuts-refusal-in-use = قد يكون هذا الاختصار مستخدمًا من تطبيق آخر أو من النظام. جرّب غيره.
shortcuts-refusal-unavailable = الاختصارات العامة غير متاحة على سطح المكتب هذا. افتح Ibis من مشغّل التطبيقات.
shortcuts-refusal-not-saved = الاختصار يعمل الآن، لكن تعذّر على Ibis حفظه للمرة القادمة. حاول مرة أخرى.
shortcuts-command-format = تغيير التنسيق
shortcuts-command-pin = إبقاء النافذة مفتوحة
shortcuts-command-keys = اختصارات لوحة المفاتيح
shortcuts-command-more = المزيد
shortcuts-command-change-last = تغيير آخر استشهاد
shortcuts-command-source = فتح المصدر
shortcuts-command-settings = الإعدادات
shortcuts-command-quit = إنهاء Ibis
shortcuts-picker-conflict = هذا الاختصار مستخدم لـ «{ $command }». جرّب اختصارًا آخر.
shortcuts-region-search = أثناء البحث
shortcuts-region-library = مكتبتك
shortcuts-region-picker = في أي مكان في Ibis
shortcuts-region-menu = في قائمة
shortcuts-region-system = فتح Ibis
shortcuts-region-citation = في الاستشهاد
shortcuts-action-change-last = تعديل الاستشهاد الذي لصقته للتو
shortcuts-action-source = فتح المصدر
shortcuts-action-choose-source = اختيار مكتبة
shortcuts-action-reveal-source = إظهار المصدر في مجلده
shortcuts-action-open = من أي تطبيق
shortcuts-action-copy = لصق الاستشهاد أو نسخه
shortcuts-action-paste = لصق الاستشهاد
shortcuts-action-copy-only = نسخ الاستشهاد
shortcuts-action-add = إضافة مرجع آخر
shortcuts-action-move = التنقل بين النتائج
shortcuts-action-pick = لصق نتيجة أو نسخها برقمها
shortcuts-action-peek = لمحة عن المرجع المحدد
shortcuts-action-escape = الرجوع أو الإخفاء
shortcuts-action-format = تغيير التنسيق
shortcuts-action-more = مزيد من الإجراءات
shortcuts-action-pin = إبقاء النافذة مفتوحة
shortcuts-action-keys = إظهار اختصارات لوحة المفاتيح
shortcuts-action-settings = فتح الإعدادات
shortcuts-action-quit = إنهاء Ibis
shortcuts-action-undo = تراجع
shortcuts-action-text-size = تكبير النص أو تصغيره
shortcuts-action-citation-move = التنقل بين المراجع
shortcuts-action-citation-remove = إزالة المرجع
shortcuts-action-citation-reorder = نقل المرجع
shortcuts-action-citation-open = إظهار تفاصيله
shortcuts-action-citation-focus = الانتقال إلى الاستشهاد
shortcuts-action-inspector = إظهار تفاصيل الاستشهاد
shortcuts-action-send = الإرسال إلى محررك
shortcuts-action-menu-move = التنقل في القائمة
shortcuts-action-menu-ends = الانتقال إلى أول عنصر أو آخره
shortcuts-action-menu-choose = اختيار العنصر
shortcuts-action-menu-leave = مغادرة القائمة والعودة إلى البحث
shortcuts-or = أو
# Keys named by a word, as the system's own menus and keyboards write them.
key-enter = Enter
key-escape = Esc
key-home = Home
key-end = End
key-tab = Tab
key-space = مسافة
key-backspace = Backspace
key-delete = Delete
key-plus = زائد
key-minus = ناقص
key-ctrl = Ctrl
key-alt = Alt
key-shift = Shift
key-super = Super
key-click = نقرة
key-click-held = نقرة

about-automatic = التحقق من وجود تحديثات تلقائيًا
about-automatic-description = يقرأ ملف تحديث صغيرًا من GitHub بضع مرات في اليوم.
about-check = التحقق من وجود تحديثات
about-check-label = التحقق من وجود تحديثات
about-check-description = معرفة ما إذا كان إصدار أحدث من Ibis جاهزًا للتثبيت.
settings-advanced-title = متقدم
settings-reset-words = إعادة تعيين افتراضي استعادة استخدام الافتراضي
about-checking = جارٍ التحقق…
about-check-failed = تعذّر على Ibis التحقق من وجود تحديثات. حاول مرة أخرى.
about-check-newest = لديك أحدث إصدار.
about-check-busy = يتحقق Ibis بالفعل، أو أن هناك تحديثًا ينتظر ردك.
about-copied = تم النسخ
about-copy-failed = تعذّر نسخ الإصدار.
about-version = الإصدار { $version }
about-mission = اختصار واحد للاستشهاد من مكتبتك في أي تطبيق.
about-updates = التحديثات
about-links = روابط
about-website = الموقع الإلكتروني
about-source = الشيفرة المصدرية
about-problem = الإبلاغ عن مشكلة
about-licences = التراخيص
about-free = برنامج حر بموجب رخصة AGPL 3.0
about-unavailable = هذا الإصدار لا يحدّث نفسه.


# Settings window pages and visual choices.
settings-look = المظهر
settings-citations = الاستشهادات
attachment-text-title = نص المرفقات
attachment-text-label = السماح لتطبيقات الذكاء الاصطناعي بقراءة نص المرفقات
attachment-text-description = يخص هذا المفتاح تطبيقات الذكاء الاصطناعي المتصلة: يمكنها قراءة نص المرفقات مع أرقام الصفحات، لا الملفات نفسها. ولا تحتاج إليه الطرفية التي تشغّل ibis.
attachment-text-words = مرفق مرفقات النص الكامل نص PDF
attachment-text-empty = لا توجد مكتبة مفتوحة.
attachment-text-loading = جارٍ قراءة خيارات المكتبة…
attachment-text-outside-label = السماح بنص المرفقات خارج مجلد المكتبة
attachment-text-outside-description = يسمح لسطر الأوامر وتطبيقات الذكاء الاصطناعي بقراءة ملفات المرفقات خارج مجلد ملف المكتبة أو مجلد تخزين Zotero على هذا الكمبيوتر.
settings-search = البحث في الإعدادات
settings-no-results = لا إعدادات تطابق «{ $query }»
settings-results = نتائج البحث
settings-results-count = { $count ->
    [zero] لا نتائج
    [one] نتيجة واحدة
    [two] نتيجتان
    [few] { $count } نتائج
    [many] { $count } نتيجةً
   *[other] { $count } نتيجة
    }
settings-read-failed = تعذّر على Ibis قراءة إعداداتك؛ أعد فتح الإعدادات للمحاولة مرة أخرى.
settings-forget-failed = تعذّر على Ibis نسيان تنسيق التطبيق؛ يظهر الخيار المحفوظ.
settings-open-failed = تعذّر على Ibis فتح ملف الإعدادات.
settings-choice-failed = تعذّر على Ibis حفظ هذا الخيار؛ تظهر القيمة المحفوظة.
settings-startup-words = بدء التشغيل تشغيل إعادة تشغيل تسجيل الدخول
settings-paste-words = لصق نسخ الحافظة Enter
settings-appearance-words = الوضع الداكن داكن فاتح سمة النظام
look-appearance = المظهر
look-appearance-description = اتبع النظام، أو اختر «فاتح» أو «داكن» لـ Ibis.
look-system = النظام
look-light = فاتح
look-dark = داكن
look-accent = لون التمييز
look-accent-description = لون التحديد والأزرار: لون تمييز Mac أو قرمزي Ibis.
look-accent-system =
    { $platform ->
        [macos] مطابقة Mac
       *[other] مطابقة النظام
    }
look-accent-ibis = قرمزي Ibis
look-accent-words = لون تمييز قرمزي أحمر
look-window-size = حجم النافذة
look-window-size-description = الحجم الذي تُفتح به النافذة.
look-window-small = صغير
look-window-medium = متوسط
look-window-large = كبير
look-window-custom = مخصص
look-window-custom-note = اسحب أحد أطراف النافذة لتحديد الحجم الذي تريده. يتذكره Ibis على كل شاشة.
look-text-size = حجم النص
look-text-size-description = حجم الكلمات في النافذة، بمعزل عن حجم النص في Mac.
look-default-text = افتراضي
look-larger-text = أكبر
look-sample-title = الحديقة الهادئة
look-sample-authors = Mira Vale وNoah Reed
look-sample-journal = مجلة الدراسات المتخيَّلة
citations-default = التنسيق الافتراضي

citations-default-description = يُستخدم في معالجات النصوص والبريد والمتصفحات حين لا يتذكر التطبيق تنسيقًا.
citations-recent = الاستشهادات الأخيرة
citations-recent-description = يعرض ما استشهدت به مؤخرًا في كل تطبيق. إيقافه يمحوها.
citations-recent-words = السجل الأخيرة استشهدت هنا نسيان الخصوصية
citations-editor = الاستشهاد من محررك
citations-editor-description = يتيح لمحرر على هذا الكمبيوتر فتح النافذة وتلقي الاستشهاد الذي تختاره، عبر ميزة Cite as You Write في Better BibTeX. لا تستطيع صفحات الويب استخدامه، ولا يغادر أي شيء هذا الكمبيوتر.
citations-editor-words = cayw cite as you write vs code vscode better bibtex zotero نقطة وصول منفذ port url محرر
citations-editor-address = العنوان لمحررك
citations-editor-address-description = الصق هذا في إعداد عنوان URL لميزة Cite as You Write في المحرر. يستطيع أي شخص لديه هذا العنوان فتح النافذة على هذا الكمبيوتر؛ وإيقافه ثم تشغيله يُنشئ عنوانًا جديدًا.
citations-editor-copy = نسخ العنوان
citations-editor-copied = تم النسخ
citations-editor-port-in-use = برنامج آخر يستخدم منفذ نقطة الوصول، لذا فهي متوقفة.
citations-editor-bad-setting = لا يمكن استخدام نقطة الوصول المحفوظة. أوقفها ثم شغّلها لإنشاء عنوان جديد.
citations-editor-not-saved = تعذّر تغيير نقطة الوصول. حاول مرة أخرى.

settings-shortcut-restored = تمت استعادة الاختصار الافتراضي


settings-newer-title = من إصدار أحدث من Ibis
settings-newer-description = هذا الإعداد للقراءة فقط في هذا الإصدار من Ibis.
settings-read-only = ملف الإعدادات للقراءة فقط؛ تظهر القيمة المحفوظة.

citations-key-formats = مفاتيح الاستشهاد
citations-style-formats = أنماط الاستشهاد
# Output names and descriptions that are words, not proper names; English repeats the engine's own.
output-name-pandoc-bare = Pandoc بلا أقواس
output-name-keys = المفاتيح فقط
output-name-doi-links = روابط DOI
output-name-scannable-cite = مسح RTF/ODF من Zotero
output-name-chicago-author-date = Chicago (المؤلف والتاريخ)
output-name-chicago-notes-bibliography = Chicago (الحواشي)
output-name-gb-numeric = GB/T 7714 (رقمي)
output-name-gb-author-date = GB/T 7714 (المؤلف والتاريخ)
output-name-gb-note = GB/T 7714 (الحواشي)
output-description-apa = APA (الإصدار السابع)
output-description-chicago-notes-bibliography = Chicago (الحواشي وقائمة المراجع)
output-description-mla = MLA (الإصدار التاسع)
output-description-gb-numeric = GB/T 7714-2025 (رقمي)
output-description-gb-author-date = GB/T 7714-2025 (المؤلف والتاريخ)
output-description-gb-note = GB/T 7714-2025 (الحواشي)
citations-filter-formats = تصفية التنسيقات
look-sample-title-2 = دروب عبر البستان
look-sample-title-3 = خريطة للأماكن الهادئة

# Quiet feedback, counted on this device only.
delight-ibis = Ibis
delight-tip = { $keys } يُبقي Ibis مفتوحًا بين الاستشهادات.
delight-library100 = 100 مرجع. بداية جيدة.
delight-library1000 = 1,000 مرجع. مكتبة متينة.
delight-library10000 = 10,000 مرجع. مكتبة رائعة.
delight-empty = اختر مرجعًا للصقه.
delight-loading = لا تزال مكتبتك قيد الفتح.
delight-no-library = افتح مكتبة للصق مرجع.
delight-zoom = { $percent }%
delight-citation = { $count ->
    [1] أول استشهاد
   *[other] الاستشهاد رقم { NUMBER($count) }
    }
delight-anniversary = عام كامل مع Ibis اليوم.
delight-about-words = ibis إصدار حول تحوت
canvas-hovered = عند التمرير
canvas-pressed = مضغوط
canvas-focused = تركيز لوحة المفاتيح

general-small-surprises-label = معلومات عن الطائر وإنجازات
general-small-surprises-description = معلومات عن الطائر وإنجازات وذكريات سنوية.
settings-surprises-words = مفاجآت صغيرة مفاجأة متعة بيضة عيد الفصح
delight-name = سُمّي على اسم الطائر الذي ربطه المصريون القدماء بتحوت، إله الكتابة وحافظ السجلات.
delight-fact-touch = يجد أبو منجل الأبيض طعامه باللمس وهو ينقّب في الطين.
delight-fact-scarlet = يكتسب أبو منجل القرمزي لونه من أصباغ القشريات التي يأكلها.
delight-fact-mummies = حنّط المصريون القدماء طيور أبي منجل قرابين لتحوت.
delight-fact-national = أبو منجل القرمزي طائر وطني لترينيداد وتوباغو.
delight-fact-flight = تتناوب طيور أبي منجل الأصلع الشمالي على قيادة سربها الذي يطير على شكل V.
undo-action = تراجع عن { $action }
undo-remove = إزالة { $reference }
undo-reset-window = إعادة تعيين موضع النافذة
undo-shortcut = استعادة الاختصار الافتراضي

undo-removed = تمت إزالة { $reference }

choice-use-zotero = استخدام Zotero
choice-zotero-help = مكتبة Zotero لديك، سواء كان Zotero مفتوحًا أم مغلقًا
choice-library-file = اختيار ملف مكتبة…
choice-library-file-help = { -library-formats }
choice-drop-file = أو اسحب إلى هذه النافذة ملفًا بتنسيق { -library-formats }.
keys-title = اختصارات لوحة المفاتيح
keys-close = رجوع
keys-action = الإجراء
keys-shortcut = الاختصار
shortcuts-filter = تصفية الاختصارات
shortcuts-empty = لا توجد إجراءات مطابقة

choice-use-file = استخدام { $name }
choice-back = اختيار مصدر آخر

backup-words = نسخة احتياطية نسخ احتياطي استعادة تصدير استيراد نقل ترحيل كمبيوتر جديد مزامنة iCloud OneDrive
backup-title = كمبيوتر آخر
backup-label = إنشاء نسخة احتياطية من الإعدادات
backup-description =
    { $platform ->
        [linux] ملف للاستعادة على كمبيوتر آخر. تبقى مواضع النوافذ على هذا الكمبيوتر.
       *[other] ملف للاستعادة على كمبيوتر آخر. تبقى التطبيقات التي سمحت لـ Ibis باللصق فيها ومواضع النوافذ على هذا الكمبيوتر.
    }
backup-button = نسخ احتياطي…
backup-saved = تم الحفظ في { $file }.
restore-label = استعادة الإعدادات
restore-description = استخدم إعدادات نسخة احتياطية هنا. يعرض Ibis التغييرات أولًا ويحتفظ بالإعدادات الحالية باسم settings.json.bak.
restore-button = استعادة…
restore-from = نسخة احتياطية من { $system }، أنشأها Ibis { $version }
restore-same = لدى هذا الكمبيوتر هذه الإعدادات بالفعل.
restore-confirm = استعادة وإعادة التشغيل
restore-cancel = إلغاء
restore-restarting = جارٍ إعادة تشغيل Ibis بالإعدادات المستعادة…
backup-change = { $setting }: { $from } ← { $to }
backup-note-library = ملف المكتبة في النسخة الاحتياطية غير موجود على هذا الكمبيوتر. يُحتفظ به، وسيجده Ibis متى توفر الملف.
backup-note-shortcut = لا تتضمن النسخة الاحتياطية اختصارًا لفتح Ibis على هذا النظام، لذا يبقى اختصار هذا الكمبيوتر.
backup-note-repairs = ملف الإعدادات على هذا الكمبيوتر تالف أو مفقود. الاستعادة تكتبه من جديد، وتحتفظ بأي ملف تالف بجانبه.
backup-setting-library = المكتبة
backup-setting-larger-text = نص أكبر
backup-setting-shortcut = اختصار الفتح على { $system }
backup-value-default = افتراضي
backup-value-on = تشغيل
backup-value-off = إيقاف
backup-value-zotero = Zotero، مكتبتي
backup-value-zotero-many = مكتبات Zotero: { $count }
backup-value-zotero-group = مجموعة Zotero { $id }
backup-problem-not-downloaded = لم يتم تنزيل النسخة الاحتياطية إلى هذا الكمبيوتر بعد. دع مجلد السحابة ينزّلها، ثم حاول مرة أخرى.
backup-problem-unreadable = تعذّر على Ibis قراءة هذا الملف. لم يتغير شيء.
backup-problem-not-a-backup = هذا الملف ليس نسخة احتياطية من إعدادات Ibis. لم يتغير شيء.
backup-problem-settings-file = هذا ملف إعدادات Ibis نفسه، وليس نسخة احتياطية. اختر ملف نسخة احتياطية بدلًا منه.
backup-problem-newer = هذه النسخة الاحتياطية من إصدار أحدث من Ibis. حدّث Ibis، ثم حاول مرة أخرى.
backup-problem-invalid = هذه النسخة الاحتياطية تالفة. لم يتغير شيء.
backup-problem-settings-unusable = لا يستطيع Ibis استخدام ملف إعداداته بحالته الحالية. لم يتغير شيء.
backup-problem-busy = نسخة أخرى من Ibis تحفظ الإعدادات. انتظر حتى تنتهي، ثم حاول مرة أخرى. لم يتغير شيء.
backup-problem-changed = تغيّرت الإعدادات بعد عرض النسخة الاحتياطية. اخترها مرة أخرى.
backup-problem-not-saved = تعذّر على Ibis الحفظ. لم يتغير شيء.
## The picker, setup, menus, notices and receipts.

reader-show = إظهار PDF
reader-hide = إخفاء PDF
reader-cite = الاستشهاد بهذه الصفحة
reader-page-in-view = الصفحة المعروضة
reader-thumbnails = الصفحات
reader-pages = صفحات ملف PDF
reader-region = صفحات { $title }
reader-unknown = صفحة PDF رقم { $index } من { $count } · بلا رقم صفحة مطبوع
reader-ask = ما الرقم المطبوع على صفحة PDF رقم { $index }؟
reader-use-nearby = استخدام للصفحات المجاورة
peek-label = تفاصيل { $title }
peek-container = منشور في
peek-year = السنة
peek-type = النوع
peek-key = مفتاح الاستشهاد
citation-details-page = الصفحة
locator-act = فصل مسرحي
locator-appendix = ملحق
locator-article-locator = مادة
locator-book = كتاب
locator-canon = قانون
locator-chapter = فصل
locator-column = عمود
locator-elocation = موقع
locator-equation = معادلة
locator-figure = شكل
locator-folio = ورقة
locator-issue = عدد
locator-line = سطر
locator-note = ملاحظة
locator-opus = عمل
locator-page = صفحة
locator-paragraph = فقرة
locator-part = جزء
locator-rule = قاعدة
locator-scene = مشهد
locator-section = قسم
locator-sub-verbo = تحت كلمة
locator-supplement = تكملة
locator-table = جدول
locator-timestamp = الوقت
locator-title-locator = عنوان
locator-verse = آية
locator-version = إصدار
locator-volume = مجلد
citation-details-locator-type = نوع الموضع
locator-short-page = ص
locator-short-chapter = ف
locator-short-section = §
locator-short-paragraph = ¶
locator-short-line = س
locator-short-verse = آ
locator-short-volume = مج
locator-short-book = ك
locator-short-part = ج
locator-short-figure = ش
locator-short-table = جدول
locator-short-note = ملاحظة
locator-short-appendix = ملحق
locator-short-article-locator = م
locator-short-canon = قانون
locator-short-column = ع
locator-short-elocation = موقع
locator-short-equation = معادلة
locator-short-folio = ورقة
locator-short-issue = عدد
locator-short-opus = عمل
locator-short-rule = قاعدة
locator-short-scene = مشهد
locator-short-sub-verbo = تحت كلمة
locator-short-supplement = تكملة
locator-short-timestamp = عند
locator-short-title-locator = عنوان
locator-short-version = إصدار
locator-short-act = فصل مسرحي
citation-details-add-prefix = نص قبله
citation-details-add-suffix = نص بعده
citation-token-remove = إزالة { $reference }
citation-details-locator-filter = ابحث عن نوع
citation-details-locator-placeholder = أضف قيمة
key-move-left = نقل إلى اليسار
key-move-right = نقل إلى اليمين
key-edit-citation = إظهار تفاصيل الاستشهاد
key-go-to-citation = الانتقال إلى الاستشهاد
picker-order-kept = يُحتفظ بترتيبك
picker-your-order = ترتيبك
citation-details-prefix = نص قبله
citation-details-suffix = نص بعده
citation-details-none = لا شيء
citation-details-page-placeholder = أضف صفحة
citation-details-page-hint = صفحة أو نطاق أو عدة صفحات، كما هي مطبوعة
citation-details-form = طريقة العرض
citation-details-examples-unavailable = الأمثلة غير متاحة
citation-details-edit = تعديل { $reference }
citation-details-details = تفاصيل استشهاد { $reference }
operation-failures-recent-history = تعذّر نسيان الاستشهادات الأخيرة. حاول مرة أخرى.
operation-failures-project = تعذّر تغيير ملف ‎.bib المستخدم للمطابقة. حاول مرة أخرى.
operation-failures-row-context = تعذّر فتح قائمة الصف. Enter ينسخ، وShift+Enter يضيف.
operation-failures-status = تعذّر على Ibis التحقق من مكتبتك. اختر الملف مرة أخرى.
operation-failures-shortcut = تعذّر التحقق من الاختصار. افتح Ibis من شريط القوائم أو علبة النظام.
operation-failures-choose = لم تُفتح نافذة اختيار الملف. حاول مرة أخرى.
operation-failures-drop-refused = أفلت ملفًا واحدًا في كل مرة ({ -library-formats }).
operation-failures-zotero = تعذّر على Ibis بدء قراءة Zotero. حاول مرة أخرى.
operation-failures-samples = تعذّر على Ibis تغيير المكتبة النموذجية. حاول مرة أخرى.
operation-failures-zotero-groups = تعذّر على Ibis عرض مجموعاتك في Zotero. تأكد من أن Zotero مفتوح، ثم حاول مرة أخرى.
operation-failures-reload = تعذّر على Ibis إعادة تحميل مكتبتك. اختر الملف مرة أخرى.
operation-failures-hide = تعذّر على Ibis إخفاء هذه النافذة. حاول مرة أخرى.
operation-failures-pin = تعذّر على Ibis تغيير التثبيت. حاول مرة أخرى.
operation-failures-preferences = تعذّر حفظ اختياراتك في النافذة. ستبقى سارية حتى إعادة التشغيل.
operation-failures-window-size = تعذّر على Ibis حفظ حجم النافذة. سيبقى كما هو حتى تُنهي Ibis.
operation-failures-quit = تعذّر إنهاء Ibis. حاول مرة أخرى.
operation-failures-update-events = تقدّم التحديث غير متاح.
operation-failures-picker-events = تعذّر على Ibis تركيز مربع البحث. أعد تشغيل Ibis لتحاول مرة أخرى.
operation-failures-library-events = قد لا تتحدّث مكتبتك تلقائيًا. استخدم «إعادة تحميل المكتبة» أو أعد تشغيل Ibis.
operation-failures-find-sources = تعذّر على Ibis البحث عن مكتبات هنا. اختر ملفًا بدلًا من ذلك.
operation-failures-open-found = تعذّر فتح هذا الملف. اختره مرة أخرى من القائمة الجديدة.
operation-failures-settings = تعذّر فتح الإعدادات. حاول مرة أخرى.
operation-failures-app-choices = تلفت اختيارات التطبيقات. اختر «لصق» أو «نسخ فقط» مرة أخرى.
operation-failures-app-read = تعذّر على Ibis معرفة هذا التطبيق. سيُطبَّق إعدادك الافتراضي.
operation-failures-app-choice = تعذّر على Ibis حفظ الاختيار لهذا التطبيق. حاول مرة أخرى.
operation-failures-app-delivery = حفظ Ibis التنسيق دون «لصق» أو «نسخ فقط». اختره مرة أخرى.
setup-sources-title = أين توجد مراجعك؟
setup-sources-line =
    { $platform ->
        [linux] يجد Ibis المرجع وينسخ استشهاده لتلصقه حيث تكتب. يقرأ مكتبتك فقط ولا يغيّرها أبدًا.
       *[other] يجد Ibis المرجع ويلصق استشهاده حيث تكتب. يقرأ مكتبتك فقط ولا يغيّرها أبدًا.
    }
setup-return-hint = بعد ذلك، افتح Ibis من أي تطبيق بالضغط على { $shortcut }.
setup-shortcut-unavailable = الاختصار غير متاح. افتح Ibis من شريط القوائم أو علبة النظام.
setup-no-shortcut = لم يُعيَّن اختصار.
setup-open-without-shortcut = افتح Ibis من شريط القوائم أو علبة النظام.
setup-opening-line = جارٍ قراءة مكتبتك. يمكن أن يبقى Zotero مغلقًا.
setup-samples = تجربة مراجع نموذجية
setup-samples-help = ثلاثة مراجع وهمية للتجربة
setup-sample-label = نموذج
setup-sample-title = مراجع نموذجية
setup-sample-search-help = ابحث عن «lantern»، واختر مرجع «Lantern gardens»، ثم اضغط Enter.
setup-own-library = استخدام مكتبتك
setup-own-library-help = اختر Zotero أو ملف مكتبة بدلًا من النماذج
setup-empty-line = أضف إليها مراجع، أو اختر مكتبة أخرى.
setup-zotero = Zotero
setup-find-libraries = البحث عنها في هذا الكمبيوتر
setup-found-in = عُثر عليها في { $folder }
setup-choose-found = اختيار مكتبة معثور عليها
setup-found-title = مكتبات عُثر عليها في هذا الكمبيوتر
setup-find-help = Zotero وملفات ‎.bib القريبة
setup-look-again = البحث مجددًا
setup-looking = جارٍ البحث عن Zotero وملفات ‎.bib…
setup-cut-short = بعض المجلدات أكبر من أن يُبحث فيها بالكامل. اختر الملف بنفسك إن لم يكن مدرجًا.
setup-nothing-found = لم يُعثر على Zotero أو ملفات ‎.bib قريبة. اختر الملف بنفسك.
setup-missing-title = تعذّر العثور على { $name }
setup-missing-line = ربما نُقل، أو أن القرص الذي يحويه غير متصل.
setup-find = البحث عن { $name }
setup-find-file-help = اختر مكانه الحالي
setup-unreadable-title = تعذّرت قراءة { $name }
setup-zotero-closed-title = افتح Zotero لقراءة مكتبتك
setup-zotero-closed-line = يقرأها Ibis عادةً وZotero مغلق، لكن ليس هذه المرة.
setup-zotero-missing-title = لا يجد Ibis مكتبة Zotero
setup-zotero-missing-line = إن لم يكن Zotero مثبّتًا، فثبّته وافتحه مرة واحدة. سيتابع Ibis تلقائيًا.
setup-zotero-access-off-title = Zotero لا يشارك مكتبتك
setup-zotero-access-off-waiting = سيتابع Ibis تلقائيًا بمجرد تفعيل المشاركة.
setup-open-zotero-help = يتابع Ibis تلقائيًا بمجرد فتح Zotero
setup-open-file-help = عرض الملف في تطبيقه
setup-add-references-help = أضف المراجع هناك
setup-zotero-profiles-title = أي ملف تعريف في Zotero؟
setup-zotero-profiles-line = Zotero مغلق. اختر ملف التعريف المراد قراءته.
setup-zotero-profiles-failed = تعذّر على Ibis عرض ملفات تعريف Zotero. افتح Zotero وسيتابع Ibis تلقائيًا.
setup-other-title = تعذّر فتح { $name }
setup-the-library = المكتبة
setup-try-again = إعادة المحاولة
setup-try-again-help = قراءة المكتبة مجددًا
setup-another = استخدام مكتبة أخرى
setup-another-help = Zotero أو ملف مكتبة آخر
session-menu = الاستشهادات منذ الفتح…
session-title = الاستشهادات منذ الفتح
session-about = ما نسخته باستخدام Ibis منذ فتحه، أو منذ إعادة تعيين هذه القائمة، في كل التطبيقات. لا يرى Ibis مستندك، لذا طابق القائمة معه.
session-empty = لا شيء بعد. تظهر هنا المراجع التي تنسخها.
session-close = رجوع
session-remove = إزالة
session-reset = إعادة تعيين القائمة
session-undo-reset = تراجع عن إعادة تعيين القائمة
session-full = القائمة ممتلئة، لذا لم تُضف المراجع اللاحقة.
session-copy = نسخ قائمة المراجع
session-save = حفظ…
session-copied = نُسخت قائمة المراجع.
session-changed = تغيّرت القائمة. راجعها، ثم حاول مرة أخرى.
session-not-read = تعذّرت قراءة المراجع. حاول مرة أخرى.
session-not-formatted = تعذّرت كتابة هذه المراجع كقائمة مراجع. جرّب نمطًا آخر.
session-not-copied = تعذّر نسخ قائمة المراجع. حاول مرة أخرى.
session-not-saved = تعذّر حفظ قائمة المراجع. حاول مرة أخرى.
writing-title = المشاريع
writing-loading = جارٍ تحميل المشاريع…
writing-saving = جارٍ حفظ المشروع…
writing-menu = مشروع جديد…
writing-about = يجمع المشروع تنسيق الاستشهاد وقائمة مراجع اختيارية معًا. يُحفظ اختيارك لهذا التطبيق، وتسري التغييرات أينما استخدمت هذا المشروع. لا يتعرّف Ibis على المستند.
writing-session-only = لا يستطيع Ibis التعرّف على التطبيق، لذا يسري هذا المشروع فقط حتى إنهاء Ibis.
writing-close = إغلاق
writing-none = بلا مشروع
writing-empty = لا توجد مشاريع بعد. يسري تنسيق تطبيقك حتى تختار واحدًا.
writing-use = استخدام { $name }
writing-selected = المشروع المحدد
writing-selected-label = محدد
writing-format = التنسيق: { $name }
writing-bibliography = قائمة المراجع: { $name }
writing-no-bibliography = لم تُحدَّد قائمة مراجع.
writing-choose-bibliography = اختيار قائمة مراجع…
writing-remove-bibliography = إزالة قائمة المراجع
writing-create = مشروع جديد…
writing-confirm-create = إنشاء المشروع
writing-starts-with = يبدأ بـ { $format }. غيّره من قائمة التنسيق بعد إنشاء المشروع.
writing-name = اسم المشروع
writing-rename = إعادة تسمية المشروع…
writing-save-name = حفظ الاسم
writing-remove = إزالة المشروع…
writing-confirm-remove = إزالة المشروع
writing-cancel = إلغاء
writing-unsupported = لا يستطيع هذا الإصدار من التطبيق استخدام تنسيق هذا المشروع.
writing-not-read = تعذّرت قراءة المشاريع. أغلق هذه اللوحة، ثم حاول مرة أخرى.
writing-not-saved = تعذّر حفظ المشروع. لا يزال اختيارك السابق ساريًا. حاول مرة أخرى.
writing-bibliography-not-saved = تعذّر حفظ اختيار قائمة المراجع. حاول مرة أخرى.
writing-pick = يغيّر اختيارك تنسيق هذا المشروع.
project-check-retry = إعادة المحاولة
project-check-locate = تحديد موقع قائمة المراجع…
picker-choose-project = المطابقة مع ملف ‎.bib…
picker-choose-other-project = المطابقة مع قائمة مراجع أخرى…
picker-open-zotero = فتح Zotero
picker-show-in-zotero = إظهار في Zotero
picker-source-changed = تغيّرت المكتبة؛ حدّد المرجع مرة أخرى.
picker-search-label = البحث في المراجع
picker-search-placeholder = ابحث في المراجع…
picker-drag-search-bar = اسحب حافة شريط البحث لتحريك النافذة
picker-search-error = تعذّر على Ibis البحث في مكتبتك. استخدم «إعادة تحميل المكتبة»، ثم حاول مرة أخرى.
picker-copy-error = لم يُنسخ شيء. حاول مرة أخرى، أو استخدم «إعادة تحميل المكتبة».
picker-snapshot-changed = تغيّرت المكتبة. ابحث مجددًا وأعد تحديد المراجع قبل النسخ.
picker-shared-key = لأكثر من مرجع المفتاح { $key }، لذا لا يمكن الاستشهاد به عبر المفتاح. أعطِ أحدها مفتاحًا مختلفًا في Zotero أو في ملف ‎.bib.
picker-using-shorthand = يُستخدم { $value }
picker-apply = تطبيق
# $word: a word that finds the person’s own cited reference, never a sample one.
picker-shorthand-hint = «{ $word } 417» لصفحة، و«see { $word }» لـ «see».
picker-kept-words = أُبقي النص كلمات بحث. لتحديد صفحة، أضف فاصلة: «room, 22».
picker-practice-offer = جرّب اللصق هنا أولًا: اضغط Enter.
# $citation: exactly what Enter would paste.
picker-practice-pasted = لُصق { $citation } هنا. بعد ذلك، يلصق Enter في تطبيقك.
picker-search-as-typed = البحث كما كُتب
picker-apply-locator = تطبيق الموضع على { $key }
picker-copied = تم النسخ
picker-key-formats = تنسيقات المفاتيح
picker-formatted-styles = أنماط الاستشهاد
picker-share-formats = مشاركة المراجع
picker-filter-formats = تصفية التنسيقات
picker-format-for-app = { $format } · { $app }
picker-format-this-paste = { $format } · هذه المرة فقط
picker-format-remembered = محفوظ لـ { $app }.
picker-format-suggested = مقترح لـ { $app }.
picker-format-app-default = لا يوجد تنسيق محفوظ لـ { $app }، لذا يسري تنسيقك الافتراضي.
picker-format-no-app = لا يعرف Ibis التطبيق المقصود، لذا يسري تنسيقك الافتراضي.
picker-format-held = يسري { $format } على هذه المرة فقط.
picker-pick-replaces = اختيارك يحلّ محله.
picker-pick-remembered = يُحفظ اختيارك لهذا التطبيق.
picker-pick-default = يصبح اختيارك هو الافتراضي.
picker-pick-this-paste = يسري اختيارك على هذه المرة فقط.
picker-pick-to-remember = اختر تنسيقًا ليُحفظ لهذا التطبيق.
picker-pick-to-set-default = اختر تنسيقًا لجعله الافتراضي.
picker-for-app = لـ { $app }
picker-pick-heading-default = التنسيق الافتراضي
picker-pick-heading-this-paste = هذه المرة فقط
picker-paste-from-now-on = لصق في { $app } من الآن فصاعدًا
picker-copy-only = نسخ فقط
picker-preview = معاينة
picker-citation-details = تفاصيل الاستشهاد
picker-details = التفاصيل
picker-citation-warnings = تحذيرات الاستشهاد
picker-window-reset = أُعيد تعيين موضع النافذة.
picker-undo = تراجع
picker-notices = التنبيهات
picker-undo-failed = تعذّر التراجع. حاول مرة أخرى.
picker-preview-pending = جارٍ التنسيق…
picker-preview-error = تعذّر تنسيق هذا الاستشهاد. اختر نمطًا آخر، أو انسخ مفاتيح الاستشهاد.
picker-preview-refused = تعذّر تجهيز هذا الاستشهاد. أغلق Ibis وأعد فتحه لتحاول مرة أخرى.
picker-preview-style-error = تعذّر التنسيق بنمط { $style }. اختر نمطًا آخر، أو انسخ المفاتيح.
picker-entry-only = تعتمد الأرقام في { $style } على مستندك بأكمله، لذا ينسخ Ibis مُدخل المرجع بدلًا من ذلك.
picker-entry-only-short = مُدخل المرجع
picker-first-note = تُختصر الحواشي اللاحقة للعمل نفسه، وترتيبها يعتمد على مستندك بأكمله، لذا ينسخ Ibis الحاشية الأولى كاملة.
picker-first-note-short = الحاشية الأولى كاملة
picker-shelf-stale = تغيّرت المكتبة. جارٍ قراءتها مجددًا…
picker-dismiss = تجاهل
picker-dismiss-warnings = تجاهل تحذيرات النسخ
picker-copy-warnings = تحذيرات النسخ
picker-citation-selection = تحديد الاستشهاد
picker-selected-references = المراجع المحددة
picker-references = المراجع
picker-search-results = نتائج البحث
picker-results = النتائج
picker-no-date = بلا تاريخ
picker-added-to-citation = أُضيف إلى الاستشهاد
picker-add-named = إضافة { $name } إلى الاستشهاد
picker-remove-named = إزالة { $name } من الاستشهاد
picker-best-matches = أفضل التطابقات
picker-pasted-list = القائمة الملصقة
picker-paste-into = لصق في { $app } · { $output }
picker-bar-paste = لصق في { $app }
picker-bar-paste-entries =
    { $count ->
        [zero] لا مُدخلات للصق في { $app }
        [one] لصق مُدخل في { $app }
        [two] لصق مُدخلين في { $app }
        [few] لصق { $count } مُدخلات في { $app }
        [many] لصق { $count } مُدخلًا في { $app }
       *[other] لصق { $count } مُدخل في { $app }
    }
picker-bar-copy =
    { $count ->
        [zero] لا استشهادات للنسخ
        [one] نسخ
        [two] نسخ استشهادين
        [few] نسخ { $count } استشهادات
        [many] نسخ { $count } استشهادًا
       *[other] نسخ { $count } استشهاد
    }
picker-bar-copy-entries =
    { $count ->
        [zero] لا مُدخلات للنسخ
        [one] نسخ مُدخل
        [two] نسخ مُدخلين
        [few] نسخ { $count } مُدخلات
        [many] نسخ { $count } مُدخلًا
       *[other] نسخ { $count } مُدخل
    }
picker-bar-send = إرسال إلى المحرر
picker-bar-change = تغيير الاستشهاد
picker-bar-practice = الصق هنا للتجربة
picker-bar-refused = لم يُلصق أو يُنسخ شيء. { $reason }
picker-last-outcome = آخر نتيجة: { $outcome }
picker-closest-matches = أقرب التطابقات
picker-closest-match = تطابق تقريبي
picker-spelling-match = تطابق إملائي
picker-closest-match-warning = «{ $title }» تطابق تقريبي: تطابقت بعض كلمات البحث فقط.
picker-no-matches = لا مراجع مطابقة
picker-no-matches-hint = جرّب اسم عائلة أو سنة أو كلمات أقل.
picker-filter-obstacle = لا نتائج مع هذه التصفية
picker-filter-obstacle-hint = وسم أو سنة أو حقل في بحثك لا يُبقي أي نتيجة.
picker-remove-filter = إزالة التصفية
picker-scope-obstacle-hint = بعض مكتبات Zotero لديك مستبعدة من هذا البحث.
picker-not-in-libraries = ليس في مكتباتك
picker-not-in-libraries-hint = أضفه إلى مكتبتك، ثم ابحث مجددًا.
picker-clear-search = مسح البحث
picker-search-all-libraries = البحث في كل مكتبات Zotero
picker-type-to-find = اكتب للعثور على مرجع
picker-type-to-find-hint = ابحث بالعنوان أو المؤلف أو السنة أو مفتاح الاستشهاد أو DOI.
picker-more = المزيد
picker-keep-my-order = الإبقاء على ترتيبي
picker-change-last-citation = تغيير آخر استشهاد
picker-paste-reference-list = لصق قائمة المراجع
picker-copy-reference-list = نسخ قائمة المراجع
picker-reference-list-title = قائمة المراجع
# $app: the app's own name. The list holds this app's citations from this run of Ibis.
picker-reference-list-help =
    { $count ->
        [zero] لم تستشهد بأي مرجع في { $app } منذ فتح Ibis
        [one] المرجع الذي استشهدت به في { $app } منذ فتح Ibis
        [two] المرجعان اللذان استشهدت بهما في { $app } منذ فتح Ibis
        [few] { $count } مراجع استشهدت بها في { $app } منذ فتح Ibis
        [many] { $count } مرجعًا استشهدت بها في { $app } منذ فتح Ibis
       *[other] { $count } مرجع استشهدت بها في { $app } منذ فتح Ibis
    }
picker-forget-recent = نسيان الاستشهادات الأخيرة
picker-change-citation = تغيير الاستشهاد · { $format }
picker-keep-open = إبقاء النافذة مفتوحة
picker-keep-open-menu = إبقاء النافذة مفتوحة
picker-change-format = تغيير التنسيق…
picker-drag-citation = اسحب الاستشهاد إلى أي تطبيق
picker-more-title = مزيد من الإجراءات ({ $keys })
picker-use-zotero = استخدام Zotero
picker-library-region = المكتبة
picker-picker-actions = إجراءات النافذة
picker-library-status = حالة المكتبة
picker-reload-library = إعادة تحميل المكتبة
picker-choose-zotero-profile = اختيار ملف تعريف Zotero…
picker-zotero-profile = ملف تعريف Zotero
picker-choose-library-menu = اختيار المكتبة…
picker-format-label = تنسيق الاستشهاد
picker-quit-label = إنهاء Ibis
picker-settings = الإعدادات…
picker-zotero-libraries = مكتبات Zotero
picker-search-in = البحث في
picker-all-libraries = كل المكتبات
picker-my-library = مكتبتي
picker-group-library = المجموعة { $id }
picker-keep-one-library = أبقِ مكتبة Zotero واحدة على الأقل مفعّلة.
key-title = اختصارات لوحة المفاتيح
key-close = إغلاق
key-footer = المفاتيح
key-copy = نسخ
key-copy-as = نسخ بتنسيق { $format }
key-copy-several-as = نسخ { $count } بتنسيق { $format }
key-add-to-citation = إضافة إلى الاستشهاد
key-remove-from-citation = إزالة من الاستشهاد
key-close-menu = إغلاق القائمة
key-clear-search = مسح البحث
key-hide = إخفاء Ibis
library-live-updates-off = التحديثات المباشرة غير متاحة. أعد تحميل المكتبة بعد تعديلها.
row-et-al = وآخرون
copied-notice-title = تم النسخ
copied-notice-plugin-note = إذا كان هذا المستند يستخدم إضافة مدير مراجع (Zotero وMendeley وEndNote وCitavi وPaperpile وغيرها)، فأدرج الاستشهاد عبر الإضافة.
copied-notice-source = Ibis
copied-notice-notice-failed = تعذّر عرض تنبيه «تم النسخ».
# Names inside these sentences arrive isolated (⁨…⁩), so a right-to-left name keeps its place.
reader-indicator = ص { $printed } · صفحة PDF { $index } من { $count }
reader-pdf-page = PDF { $index }
setup-found-on =
    { $platform ->
        [macos] موجود على هذا الـ Mac
        [windows] موجود على هذا الكمبيوتر
       *[other] موجود على هذا الكمبيوتر
    }
setup-zotero-access-off = لقراءة مراجعك، فعّل في Zotero › الإعدادات › متقدم الخيار «السماح للتطبيقات الأخرى على هذا الكمبيوتر بالتواصل مع Zotero».
setup-zotero-open = Zotero مفتوح.
setup-zotero-installed = Zotero مثبّت على هذا الكمبيوتر.
setup-bib-found =
    { $count ->
        [zero] لم يُعثر على ملفات بصيغة ‎.bib.
        [one] عُثر على ملف واحد بصيغة ‎.bib.
        [two] عُثر على ملفين بصيغة ‎.bib.
        [few] عُثر على { $count } ملفات بصيغة ‎.bib.
        [many] عُثر على { $count } ملفًا بصيغة ‎.bib.
       *[other] عُثر على { $count } ملف بصيغة ‎.bib.
    }
session-remove-named = إزالة { $name }
session-undo-remove = تراجع عن الإزالة
session-undo-remove-named = تراجع عن إزالة { $name }
session-absent =
    { $count ->
        [zero] لا مراجع خارج المكتبة المفتوحة.
        [one] مرجع واحد ليس في المكتبة المفتوحة، لذا استُبعد.
        [two] مرجعان ليسا في المكتبة المفتوحة، لذا استُبعدا.
        [few] { $count } مراجع ليست في المكتبة المفتوحة، لذا استُبعدت.
        [many] { $count } مرجعًا ليست في المكتبة المفتوحة، لذا استُبعدت.
       *[other] { $count } مرجع ليست في المكتبة المفتوحة، لذا استُبعدت.
    }
session-left-out = استُبعدت لأن هذا النمط لا يستطيع كتابتها: { $names }.
session-bibliography-in = قائمة المراجع بنمط { $style }
session-saved = تم حفظ { $name }.
writing-remove-question = إزالة { $name } من Ibis؟ يبقى ملف قائمة المراجع في مكانه.
writing-remembered = تم الحفظ لـ { $name }.
picker-project-state =
    { $state ->
        [in] في { $name }
        [missing] غير موجود في { $name }
        [could-not-check] تعذّر التحقق من { $name }
        [repeated] المفتاح مكرر
       *[may-name-another] قد يشير المفتاح إلى مرجع آخر
    }
picker-project-lines =
    { $count ->
        [zero] الأسطر { $lines }
        [one] السطر { $lines }
        [two] السطران { $lines }
        [few] الأسطر { $lines }
        [many] الأسطر { $lines }
       *[other] الأسطر { $lines }
    }
picker-project-reading = جارٍ قراءة { $name }
picker-project-damaged = تعذّرت قراءة { $name } كاملًا، لذا لا يُعدّ أي مرجع مفقودًا.
picker-project-failed = تعذّرت قراءة { $name }.
picker-project-checking = جارٍ المطابقة مع { $name }
picker-project-check-lines =
    { $count ->
        [zero] تحقق من الأسطر { $lines }.
        [one] تحقق من السطر { $lines }.
        [two] تحقق من السطرين { $lines }.
        [few] تحقق من الأسطر { $lines }.
        [many] تحقق من الأسطر { $lines }.
       *[other] تحقق من الأسطر { $lines }.
    }
picker-stop-project = إيقاف المطابقة مع { $name }
picker-copy-project-entry = نسخ المُدخل لـ { $name }
picker-project-entry =
    { $outcome ->
        [copied] نُسخ مُدخل { $key } كما هو في ملفك، بما في ذلك أي حقل file أو note. راجعه، ثم أضفه إلى { $name }.
        [no-original] لا يوجد لهذا المرجع مُدخل BibTeX أصلي هنا. صدّره من مدير المراجع بالصيغة التي يستخدمها المشروع، مع الاحتفاظ بمفتاحه.
        [changed] تغيّرت المكتبة أو قائمة المراجع. ابحث مجددًا، ثم حاول مرة أخرى.
        [repeated] يتكرر هذا المفتاح في ملف المكتبة، لذا لا يستطيع Ibis معرفة أي مُدخل ينسخ.
       *[dependency] يعتمد هذا المُدخل على crossref أو xdata أو @string لا يمكن نسخه معه. صدّره من مدير المراجع، مع الاحتفاظ بمفتاحه.
    }
picker-editor-waiting = استشهاد لمحررك بتنسيق { $format }. Esc للإلغاء.
picker-editor-asking = استشهاد لمحررك. Esc للإلغاء.
picker-send = إرسال · { $format }
picker-delivery-editor = المحرر
picker-delivery-clipboard = الحافظة
picker-delivery-paste = لصق تلقائي
picker-find-source = البحث عن { $name }…
picker-find-source-unnamed = بحث…
picker-show-source-folder =
    { $platform ->
        [macos] إظهار في Finder
       *[other] إظهار في المجلد
    }
picker-open-source = فتح { $name }
picker-open-library = فتح المكتبة
picker-source-open-failed =
    { $source ->
        [zotero] تعذّر فتح Zotero. حاول مرة أخرى.
       *[file] تعذّر فتح ملف المصدر. حاول مرة أخرى.
    }
picker-warning-count =
    { $count ->
        [zero] لا تحذيرات
        [one] تحذير واحد
        [two] تحذيران
        [few] { $count } تحذيرات
        [many] { $count } تحذيرًا
       *[other] { $count } تحذير
    }
picker-copy-citations =
    { $count ->
        [zero] لا استشهادات للنسخ
        [one] نسخ استشهاد واحد
        [two] نسخ استشهادين
        [few] نسخ { $count } استشهادات
        [many] نسخ { $count } استشهادًا
       *[other] نسخ { $count } استشهاد
    }
picker-entry-output =
    { $count ->
        [zero] لا مُدخلات { $style }
        [one] مُدخل { $style }
        [two] مُدخلان { $style }
        [few] { $count } مُدخلات { $style }
        [many] { $count } مُدخلًا { $style }
       *[other] { $count } مُدخل { $style }
    }
picker-missing-citations = ليست في مكتبتك: { $keys }
picker-matched-in =
    { $field ->
        [doi] مطابقة في DOI
        [keyword] مطابقة في الكلمات المفتاحية
        [container] مطابقة في المجلة أو الكتاب
        [abstract] مطابقة في الملخّص
        [identifier] مطابقة في ISBN أو معرّف PubMed أو arXiv
       *[other] مطابقة في { $field }
    }
picker-recent-heading =
    { $state ->
        [cited] استُشهد بها هنا مؤخرًا
       *[added] أُضيفت مؤخرًا
    }
picker-more-references = +{ $count } أخرى
picker-result-count =
    { $count ->
        [zero] لا نتائج
        [one] نتيجة واحدة
        [two] نتيجتان
        [few] { $count } نتائج
        [many] { $count } نتيجةً
       *[other] { $count } نتيجة
    }
picker-selection-count =
    { $count ->
        [zero] لم يُحدَّد أي مرجع للاستشهاد
        [one] حُدّد مرجع واحد للاستشهاد
        [two] حُدّد مرجعان للاستشهاد
        [few] حُدّد { $count } مراجع للاستشهاد
        [many] حُدّد { $count } مرجعًا للاستشهاد
       *[other] حُدّد { $count } مرجع للاستشهاد
    }
picker-shortcut-not-used = لم يُستخدم إعداد الاختصار لديك: { $reason }
picker-shortcut-unavailable = الاختصار { $shortcut } غير متاح: { $reason }
key-copy-entries =
    { $count ->
        [zero] نسخ مُدخلات { $style }
        [one] نسخ مُدخل { $style }
        [two] نسخ مُدخلَي { $style }
        [few] نسخ { $count } مُدخلات { $style }
        [many] نسخ { $count } مُدخلًا { $style }
       *[other] نسخ { $count } مُدخل { $style }
    }
library-opening = جارٍ فتح { $name }…
library-opening-unnamed = جارٍ فتح المكتبة…
library-reloading = جارٍ إعادة تحميل { $name }…
library-reloading-unnamed = جارٍ إعادة تحميل المكتبة…
library-zotero-not-read = تعذّرت قراءة { $name }. { $reason }
library-open-zotero-then-reload = افتح Zotero، ثم أعد التحميل.
library-file-not-opened = تعذّر فتح المكتبة: { $name }. { $reason } اخترها مجددًا أو أعد توصيل القرص.
library-the-chosen-file = الملف المختار
library-empty-title = لا مراجع في { $name }
library-empty-title-unnamed = لا مراجع في هذه المكتبة
library-checking = جارٍ التحقق من المكتبة…
library-details-unavailable = تفاصيل المكتبة غير متاحة
library-none-chosen = لم تُختر أي مكتبة
library-not-opened = لم تُفتح
library-moved = نُقلت
library-not-updated = لم تُحدَّث
library-no-references = لا مراجع
library-references =
    { $count ->
        [zero] لا مراجع
        [one] مرجع واحد
        [two] مرجعان
        [few] { $count } مراجع
        [many] { $count } مرجعًا
       *[other] { $count } مرجع
    }
library-skipped =
    { $count ->
        [zero] لم يُتخطَّ أي مُدخل
        [one] تم تخطي مُدخل واحد
        [two] تم تخطي مُدخلين
        [few] تم تخطي { $count } مُدخلات
        [many] تم تخطي { $count } مُدخلًا
       *[other] تم تخطي { $count } مُدخل
    }
library-to-check = { $count } للتحقق
library-age-opening = جارٍ الفتح…
library-age-reloading = جارٍ إعادة التحميل…
library-updated = حُدّثت { $ago }
library-checked-at = تم التحقق { $ago }، { $time }
library-last-read-at = آخر قراءة { $ago }، { $time }
library-zotero-unreachable = تعذّر الوصول إلى Zotero
library-file-unreadable = تعذّرت قراءة الملف
library-where = ({ $lines })
library-key-users = { $count } تستخدم المفتاح { $key }
library-others-share =
    { $count ->
        [zero] لا مُدخلات أخرى تشترك في مفتاح
        [one] مُدخل آخر يشترك في مفتاح
        [two] مُدخلان آخران يشتركان في مفتاح
        [few] { $count } مُدخلات أخرى تشترك في مفتاح
        [many] { $count } مُدخلًا آخر تشترك في مفتاح
       *[other] { $count } مُدخل آخر تشترك في مفتاح
    }
library-could-not-read =
    { $skipped ->
        [zero] تعذّرت قراءة { $count }
        [one] تعذّرت قراءته
        [two] تعذّرت قراءة { $count }
        [few] تعذّرت قراءة { $count }
        [many] تعذّرت قراءة { $count }
       *[other] تعذّرت قراءة { $count }
    }
library-skipped-because =
    { $skipped ->
        [zero] لم يُتخطَّ أي مُدخل: { $reasons }.
        [one] تم تخطي مُدخل واحد: { $reasons }.
        [two] تم تخطي مُدخلين: { $reasons }.
        [few] تم تخطي { $skipped } مُدخلات: { $reasons }.
        [many] تم تخطي { $skipped } مُدخلًا: { $reasons }.
       *[other] تم تخطي { $skipped } مُدخل: { $reasons }.
    }
library-check = تحقق من { $lines }.
library-check-library = تحقق من المكتبة.
library-line = السطر { $line }
library-line-why = السطر { $line }: { $why }
library-and-more = و{ $count } أخرى
library-lines = الأسطر { $lines } و{ $last }
library-lines-and-others = الأسطر { $lines } و{ $last } وغيرها
library-work-limit = المكتبة أكبر من أن تُقرأ كاملة.
library-cloud-synced = إنها في مجلد متزامن مع السحابة، لذا قد تصل التغييرات متأخرة.
library-zotero-newer = إصدار Zotero أحدث من الإصدار الذي اختُبر عليه هذا الإصدار من Ibis. تبدو النتائج صحيحة، لكن حدّث Ibis متى أمكنك.
library-keys-unavailable = تعذّرت قراءة مفاتيح Better BibTeX، لذا بعض العناصر بلا مفاتيح.
time-just-now = الآن
time-on = في { $date }
time-short =
    { $unit ->
        [now] الآن
        [minute] { $n } د
        [hour] { $n } س
       *[day] { $n } ي
    }
# Short word for a CSL item type, read by screen readers; other types say none.
row-item-type =
    { $type ->
        [article] مقالة
        [article-journal] مقالة
        [article-magazine] مقالة
        [article-newspaper] مقالة
        [book] كتاب
        [chapter] فصل
        [dataset] مجموعة بيانات
        [entry-dictionary] مُدخل
        [entry-encyclopedia] مُدخل
        [manuscript] مخطوطة
        [paper-conference] ورقة مؤتمر
        [patent] براءة اختراع
        [post] منشور
        [post-weblog] منشور مدونة
        [report] تقرير
        [review] مراجعة
        [review-book] مراجعة
        [software] برمجية
        [speech] محاضرة
        [standard] معيار
        [thesis] أطروحة
        [webpage] صفحة ويب
       *[other] {""}
    }
copied-notice-pasted-into = تم اللصق في { $app }
copied-notice-pasted = تم اللصق
# Every "copied, not pasted" sentence says why and ends with its fix, written
# with ⌘V; other systems show Ctrl+V.
copied-notice-reason =
    { $reason ->
        [paste-off] تم النسخ. اللصق متوقف في الإعدادات. اضغط ⌘V للصق.
        [unsupported-system] تم النسخ. لا يستطيع Ibis اللصق على هذا النظام بعد. اضغط ⌘V للصق.
        [no-target] تم النسخ. لم يكن هناك تطبيق للصق فيه. اضغط ⌘V للصق.
        [unknown-app] تم النسخ. اللصق التلقائي غير معتمد لهذا التطبيق. اضغط ⌘V للصق.
        [copy-only] تم النسخ. «نسخ فقط» مضبوط لهذا التطبيق. اضغط ⌘V للصق.
        [nonstandard-shortcut] تم النسخ. مفتاح اللصق في هذا التطبيق يؤدي أمرًا آخر. استخدم أمر اللصق الخاص به.
        [target-changed] تم النسخ. تغيّر التطبيق أو النافذة الهدف. اضغط ⌘V للصق.
        [clipboard-changed] تم النسخ. تغيّرت الحافظة منذ ذلك الحين. انسخ الاستشهاد مجددًا.
        [paste-expired] تم النسخ. انتهت مهلة اللصق. اضغط ⌘V للصق.
        [no-permission] تم النسخ. يحتاج Ibis إلى موافقتك للصق. اضغط ⌘V للصق.
        [accessibility-required] تم النسخ. يحتاج Ibis إلى إذن تسهيلات الاستخدام للصق بأمان. اضغط ⌘V للصق.
        [terminal] تم النسخ. لم يُلصق في طرفية. اضغط ⌘V للصق.
        [remote-desktop] تم النسخ. لم يُلصق في سطح مكتب بعيد. اضغط ⌘V للصق.
        [virtual-machine] تم النسخ. لم يُلصق في جهاز افتراضي. اضغط ⌘V للصق.
        [secure-field] تم النسخ. هذا الحقل يخفي ما تكتبه. اضغط ⌘V للصق.
        [secure-input] تم النسخ. الإدخال الآمن من لوحة المفاتيح مفعّل، ربما في تطبيق آخر. اضغط ⌘V للصق.
        [elevated-app] تم النسخ. لم يُلصق في تطبيق يعمل بصلاحيات المسؤول. اضغط ⌘V للصق.
        [key-release-failed] تم النسخ. تعذّر على Ibis تحرير المفاتيح التي ضغطها. اضغط Ctrl وV ثم حرّرهما للصق.
        [selection-not-shown] تم النسخ. لا يستبدل Ibis إلا نصًا عرضه عليك. اضغط ⌘V لاستبداله.
        [citation-not-selected] تم النسخ. لا يستبدل Ibis الاستشهاد إلا عند تحديده. حدّده، ثم اضغط ⌘V.
       *[paste-unavailable] تم النسخ. تعذّر على Ibis التأكد من وجهة النص. اضغط ⌘V للصق.
    }
update-title = جارٍ تحديث Ibis
app-newer-installed = إصدار أحدث من Ibis مثبّت. أنهِ Ibis وأعد فتحه لاستخدامه.
update-downloading = جارٍ التنزيل
update-installing = جارٍ التحقق والتثبيت
update-failed = لم يُثبَّت التحديث. استخدم «التحقق من وجود تحديثات» في الإعدادات › حول Ibis.
library-entries-to-check =
    { $count ->
        [zero] لا مُدخلات للتحقق
        [one] مُدخل واحد للتحقق
        [two] مُدخلان للتحقق
        [few] { $count } مُدخلات للتحقق
        [many] { $count } مُدخلًا للتحقق
       *[other] { $count } مُدخل للتحقق
    }
library-more-not-listed =
    { $count ->
        [zero] لا عناصر أخرى غير مدرجة.
        [one] عنصر آخر غير مدرج.
        [two] عنصران آخران غير مدرجين.
        [few] { $count } عناصر أخرى غير مدرجة.
        [many] { $count } عنصرًا آخر غير مدرج.
       *[other] { $count } عنصر آخر غير مدرج.
    }

citations-documents = في Word وLibreOffice والمستندات الأخرى
citations-keys = في LaTeX وMarkdown وObsidian
citations-keys-description = يُستخدم حيث تكتب مفاتيح الاستشهاد، وفي التطبيقات التي لا يعرفها Ibis.
citations-apps = تنسيقات محفوظة للتطبيقات
citations-forget = نسيان
citations-writings = المشاريع
settings-open-file = فتح ملف الإعدادات
settings-open = فتح
settings-file-description = تسري التعديلات على settings.json أثناء تشغيل Ibis.
picker-use-usual = استخدام { $format } في { $app } مجددًا
picker-stop-writing = إيقاف استخدام { $name } في { $app }

copied-notice-suggested = تم اللصق بتنسيق { $format }
copied-notice-suggestion = مقترح: { $format }
copied-notice-change = تغيير

picker-find-open = فتح PDF، أو إظهار العنصر في Zotero
shortcuts-action-find-open = فتح PDF، أو إظهار العنصر

citations-unlisted = تنسيق مخصّص

picker-format-suggestion = مقترح إلى أن تختار تنسيقًا.
## The palette: the format chooser and More (ui/Palette).
palette-back = رجوع
palette-run = تنفيذ
palette-recent = المستخدمة مؤخرًا
palette-citation = هذا الاستشهاد
palette-library = هذه المكتبة
palette-ibis = Ibis
palette-peek = لمحة عن المرجع
palette-actions-title = الإجراءات
palette-search-actions = ابحث في الإجراءات
palette-no-actions = لا توجد إجراءات مطابقة
picker-format-title = التنسيق
palette-search-formats = ابحث في التنسيقات
palette-no-formats = لا توجد تنسيقات مطابقة
palette-use-again = استخدام { $format } في { $app } مجددًا
palette-use-for-app = استخدام في { $app }
palette-use-default = استخدام في كل مكان
palette-use-this-paste = استخدام لهذه المرة فقط
palette-style-unavailable = غير متاح الآن
palette-preview-example = مثال
palette-preview-key = ما يلصقه Ibis
palette-preview-more = و{ $count } أخرى
palette-preview-unavailable = لا توجد معاينة لهذا التنسيق.
palette-words-source = فتح ملف مجلد zotero إظهار عرض مكتبة
palette-words-reveal-source = finder explorer مستكشف مجلد ملف موقع
palette-words-choose-source = مكتبة ملف bib zotero فتح تبديل تغيير
palette-words-project-entry = مراجع bib مدخل ناقص مفقود نسخ
palette-words-project-choose = مراجع bib تحقق مشروع
palette-words-project-stop = مراجع bib تحقق إيقاف
palette-words-format = نمط إخراج apa pandoc latex صيغة مشاركة تصدير bibtex doi json
palette-words-share = مشاركة تصدير إرسال نسخ مراجع
palette-words-peek = تفاصيل معاينة لمحة معلومات
palette-words-change-last = تعديل استبدال سابق استشهاد مجددا
palette-words-reference-list = قائمة مراجع ببليوغرافيا أعمال مستشهد بها مداخل لصق نسخ
palette-words-citation-focus = تركيز رمز مرجع استشهاد
palette-words-citation-edit = تعديل صفحة موضع تفاصيل استشهاد
palette-words-citation-move-left = نقل ترتيب سابق رمز مرجع استشهاد
palette-words-citation-move-right = نقل ترتيب تالي رمز مرجع استشهاد
palette-words-citation-remove = إزالة حذف رمز مرجع استشهاد
palette-words-keep-order = ترتيب فرز تسلسل
palette-words-session-references = مراجع سجل مستشهد قائمة
palette-words-forget-recent = مسح سجل خصوصية أخيرة حديثة
palette-words-pin = تثبيت عائم إبقاء مفتوح
palette-words-keys = اختصارات لوحة مفاتيح مساعدة
palette-words-settings = إعدادات تفضيلات خيارات ضبط
palette-words-quit = خروج إغلاق إيقاف إنهاء
reason-strict-citation = يتطلب الوضع الصارم استشهادًا كاملًا بلا فقد أو مشكلات في المصدر.
    .cli = تحقق من المصدر المُبلَّغ عنه أو من خيارات الاستشهاد، أو أعد المحاولة دون --strict لمراجعة النتائج.
    .agent = أصلح المصدر أو خيارات الاستشهاد، أو اطلب من المستخدم مراجعة نتائج استشهاد عادي.
    .app = تحقق من المصدر وخيارات الاستشهاد قبل إعادة المحاولة.

cli-strict-help = رفض الاستشهادات التي فيها فقد أو رفض أو مشكلات في المصدر
mcp-strict-help = رفض حالات الفقد أو الرفض أو مشكلات المصدر.
mcp-citation-hidden = يحتوي هذا الاستشهاد على أحرف مخفية ولا يمكن إرجاعه دون تغيير. اطلب من المستخدم إصلاح المصدر أو اختيار إخراج آخر.
mcp-citation-source-failed = تعذّر تحديث أحد المصادر؛ يستخدم هذا الاستشهاد آخر نسخة سليمة منه. ويسجّل الإيصال آخر قراءة ناجحة له.
mcp-citation-source-warnings = في أحد المصادر المستشهد بها تحذيرات؛ ويسجّل إيصاله عددها.
mcp-cite-description = استشهد بالمراجع (refs) المعادة من search بالترتيب، مع مواضع اختيارية. استخدم النص المُعاد كما هو، وأبلغ المستخدم بالتنبيهات وحالات الفقد والتحذيرات.

# The desktop's bundled command. Never execute another program to check its version.
command-install = تثبيت الأمر ibis
command-install-description = ابحث واستشهد من الطرفية بالمكتبة والإعدادات نفسها.
command-remove = إزالة الأمر
command-checking = جارٍ التحقق من الأمر ibis…
command-missing = غير مثبّت
command-matching = ibis { $version } — يطابق هذا التطبيق
command-different = ibis { $version } — إصدار مختلف عن هذا التطبيق
command-foreign = يُستخدم ibis آخر: { $path }. الإصدار غير متاح.
command-profile = أضف هذا السطر إلى ملف تعريف الصدفة، ثم أعد فتح الطرفية. يتحقق Ibis من PATH المتاح للتطبيق.
command-copy = نسخ
command-copied = تم النسخ
command-failed = تعذّر تغيير الأمر ibis. تحقق من أذونات المجلد ثم حاول مرة أخرى.

# Source status and reading warnings; separate from citation warnings.
source-warning-zotero-newer-than-tested = إصدار Zotero أحدث من الإصدار الذي اختُبر عليه هذا الإصدار من Ibis. تبدو النتائج صحيحة، لكن حدّث Ibis متى أمكنك.
source-warning-cloud-synced-directory = المكتبة في مجلد متزامن مع السحابة، لذا قد تتأخر التغييرات.
source-warning-citation-keys-unavailable = تعذّرت قراءة مفاتيح Better BibTeX، لذا بعض العناصر بلا مفاتيح.
source-warning-syntax = تعذّرت قراءة أحد المُدخلات، لذا يتخطاه Ibis.
source-warning-duplicate-key = أكثر من مُدخل يستخدم المفتاح أو المعرّف نفسه، لذا يتخطاها Ibis كلها.
source-warning-field = تعذّر تحويل أحد الحقول بالكامل؛ تحقق من هذا المُدخل.
source-warning-work-limit = المكتبة أكبر من أن تُقرأ كاملة.
source-warning-invalid-attachment = رابط أحد المرفقات غير صالح.
source-warning-pdfs-not-kept = في هذا المجلد ملفات PDF لم يحفظها Ibis بعد.

# Per-setting reset actions name their setting for assistive technology.
settings-use-default = استخدام القيمة الافتراضية لإعداد { $setting }
settings-default-restored = استُعيدت القيمة الافتراضية لإعداد { $setting }
settings-undo-default = استعادة القيمة الافتراضية لإعداد { $setting }
settings-details = التفاصيل…
settings-shortcut-words = لوحة مفاتيح اختصار مفتاح ربط

## Citation diagnostics carried to clients before rendering.
cli-ambiguous-suggestion = هذا المفتاح ملتبس. اختر id إحدى النتائج من هذا البحث، ثم استخدم ibis show --id ID_JSON مع المكتبات نفسها.
cli-ambiguous-search-keys = تنبيه: أُغفلت نتائج مفاتيح استشهادها ملتبسة (العدد: { $count })؛ ابحث باستخدام --json واستخدم id النتيجة.
problem-empty-selection = لا يوجد ما يُستشهد به.
problem-too-many-items = الحد الأقصى للمراجع في الاستشهاد الواحد: { $count }.
problem-invalid-locator = صحّح الصفحة أو الموضع لهذا المرجع.
    .named = الصفحة أو الموضع في { $reference } فارغ أو يحتوي على أحرف تحكم.
problem-invalid-rich-text = صحّح النص الذي قبل هذا المرجع أو بعده.
    .named = النص الذي قبل { $reference } أو بعده يحتوي على أحرف لا يستطيع Ibis كتابتها.
problem-output-too-large = الاستشهاد أكبر من أن يُكتب.
problem-unsupported-form = لا يستطيع هذا الإخراج كتابة شكل الاستشهاد المختار.
    .app-named = { $reference }: لا يمكن التعبير عن { $form } في هذا الإخراج أو هذه المجموعة.
    .cli-form = لا يمكن الاستشهاد بـ { $reference } في { $output } مع --form { $form }. جرّب --form normal.
    .cli-options = لا يمكن الاستشهاد بـ { $reference } مع { $options } في { $output }. جرّب دونها.
    .named = لا يستطيع { $output } كتابة { $reference } بهذا الشكل أو مع هذه الخيارات.
    .cli = استشهد به منفردًا، أو اختر إخراجًا آخر.
    .agent = جرّب الشكل normal، أو تنسيقًا أو نمطًا آخر.
    .app = اختر «الاسم بين قوسين»، أو استشهد به منفردًا، أو اختر إخراجًا آخر.
    .bibliography = أزل من هذه القائمة المراجع التي لا يستطيع هذا النمط كتابتها.
problem-missing-item = أُزيل هذا المرجع أثناء الاستشهاد.
    .named = تغيّر { $reference } أو أُزيل أثناء الاستشهاد.
    .cli = حاول مرة أخرى.
    .agent = ابحث مجددًا.
    .app = ابحث مجددًا.
problem-changed-item = تغيّر هذا المرجع أثناء الاستشهاد.
    .named = تغيّر { $reference } أثناء الاستشهاد.
    .cli = حاول مرة أخرى.
    .agent = ابحث مجددًا.
    .app = ابحث مجددًا.
problem-missing-key = ليس لهذا المرجع مفتاح استشهاد.
    .named = ليس لـ { $reference } مفتاح استشهاد.
    .cli = استشهد به باستخدام --style بدلًا من ذلك.
    .agent = لا يمكن لصيغة المفاتيح تسميته؛ استخدم نمطًا.
    .app = أعطه مفتاحًا في مكتبتك، أو اختر نمطًا.
problem-missing-details = ليس لهذا المرجع عنوان ولا مؤلف ولا محرر بعد، لذا لا يمكن الاستشهاد به. أضفها في مكتبته.
    .named = ليس لـ{ $reference } عنوان ولا مؤلف ولا محرر بعد.
    .cli = أضف عنوانًا أو مؤلفًا إلى مدخله، ثم استشهد به مجددًا.
    .agent = لا يمكن الاستشهاد به حتى تمنحه مكتبته عنوانًا أو مؤلفًا أو محررًا.
    .app = أضف إليه عنوانًا أو مؤلفًا في مكتبته.

problem-ambiguous-key = تشترك عدة مراجع في مفتاح الاستشهاد هذا.
    .named = تشترك عدة مراجع في مفتاح استشهاد { $reference }.
    .cli = أعطِ كلًّا منها مفتاحًا خاصًا به في مكتبتك.
    .agent = استخدم نمطًا، أو اطلب من الشخص إعطاء كل مرجع مفتاحًا خاصًا به.
    .app = أعطِ كلًّا منها مفتاحًا خاصًا به في مكتبتك.
problem-conflicting-key = لهذا المرجع مفاتيح استشهاد متعارضة.
    .named = لـ { $reference } أكثر من مفتاح استشهاد في مكتبته.
    .cli = أبقِ على واحد.
    .agent = استخدم نمطًا، أو اطلب من الشخص الإبقاء على مفتاح واحد.
    .app = أبقِ على واحد في مكتبتك، أو اختر نمطًا.
problem-invalid-key = لا يستطيع هذا الإخراج كتابة مفتاح استشهاد المرجع.
    .named = يحتوي مفتاح استشهاد { $reference } على أحرف لا يستطيع { $output } كتابتها.
    .agent = استخدم نمطًا، أو اطلب من الشخص تغيير المفتاح.
problem-unsupported-output = إخراج الاستشهاد هذا غير متاح.
    .named = لا يوجد نمط باسم { $output }.
    .cli = اعرض الأنماط باستخدام: ibis styles
    .agent = تعرضها أداة styles.
problem-invalid-locale = لغة الاستشهاد هذه غير متاحة.
    .named = لغة الاستشهاد هذه غير متاحة.
    .cli = اعرضها باستخدام: ibis styles --locales
    .agent = تعرض أداة styles اللغات المتاحة.
problem-cancelled = أُلغي الاستشهاد.
problem-formatter-unavailable = تعذّرت كتابة الاستشهاد الآن.
    .named = تعذّرت كتابة الاستشهاد الآن. حاول مرة أخرى.
problem-annotation-mismatch = ينتمي هذا التعليق التوضيحي إلى مرجع آخر.
    .named = التعليق التوضيحي المعطى لـ { $reference } ينتمي إلى مرجع آخر.
problem-not-citable = لا يمكن الاستشهاد بهذا العنصر كمرجع.
    .named = { $reference } ليس مرجعًا (كأن يكون ملاحظة)، لذا لا يمكن الاستشهاد به.
problem-not-from-zotero = لم يُقرأ هذا المرجع من Zotero.
    .named = لم يُقرأ { $reference } من مكتبة Zotero، لذا لا يستطيع Zotero العثور عليه.
    .cli = اقرأ مكتبة Zotero باستخدام --zotero، أو اختر إخراجًا آخر.
    .agent = استخدم تنسيقًا آخر، أو اطلب من الشخص الاستشهاد به من مكتبة Zotero الخاصة به.
    .app = استشهد به من مكتبة Zotero، أو اختر إخراجًا آخر.
problem-missing-doi = ليس لأحد المراجع المحددة DOI صالح للاستخدام.
    .named = ليس لـ { $reference } DOI صالح للاستخدام.
    .cli = شاركه باستخدام --format bibtex بدلًا من ذلك.
    .agent = استخدم format bibtex بدلًا من ذلك.
    .app = اختر BibTeX من قائمة تنسيق الاستشهاد.

## Native command failures.
app-failure-cli-status = تعذّرت قراءة إعداد سطر الأوامر.
app-failure-set-cli-installed = تعذّر تغيير إعداد سطر الأوامر.
app-failure-copy-cli-profile = تعذّر نسخ إعداد الصدفة.
app-failure-hide-picker = تعذّر إغلاق النافذة.
app-failure-copy-editor-endpoint = تعذّر نسخ عنوان المحرر.
app-failure-copy-version = تعذّر نسخ الإصدار.
app-failure-set-window-size = تعذّر حفظ حجم النافذة.
app-failure-undo-window-position = تعذّرت استعادة موضع النافذة.
app-failure-copy-project-entry = تعذّر نسخ مُدخل المرجع.
app-failure-finish-paste-guide = تعذّر إكمال إعداد اللصق.
app-failure-row-context = تعذّرت قراءة تفاصيل المرجع.
app-failure-find-zotero = تعذّر التحقق من Zotero.
app-failure-find-sources = تعذّر العثور على المكتبات.
app-failure-open-found-library = تعذّر فتح هذه المكتبة.
app-failure-configure-shortcuts = تعذّر فتح إعداد اختصارات لوحة المفاتيح.
app-failure-use-sample-library = تعذّر فتح المراجع النموذجية.
app-failure-use-zotero-libraries = اختر مكتبة Zotero واحدة على الأقل، دون تكرار.
app-failure-recent-references = تعذّرت قراءة المراجع الأخيرة.
app-failure-search = تعذّر إكمال البحث.
app-failure-drag-paste-app = تعذّر سحب تطبيق Ibis.

app-command-no-library = اختر مكتبة للبحث فيها.
app-project-error-invalid-name = سمِّ المشروع باسم من 1 إلى 80 حرفًا، دون أحرف تحكم.
app-project-error-duplicate-name = يوجد مشروع بهذا الاسم. اختر اسمًا آخر.
app-project-error-not-found = لم يعد ذلك المشروع متاحًا. اختر مشروعًا مجددًا.
app-project-error-too-many = يستطيع Ibis تذكّر 64 مشروعًا على الأكثر. أزل واحدًا قبل إضافة آخر.
app-project-error-invalid = تعريفات المشاريع غير صالحة. تحقق من الإعدادات.
app-project-error-changed = تغيّر المشروع أو الوجهة. افتح قائمة التنسيق مجددًا.
app-project-error-editor = طلب المحرر هذا يختار إخراجه بنفسه. أكمله قبل تغيير المشروع.
app-project-error-output = تنسيق هذا المشروع غير متاح في النافذة. اختر تنسيقًا آخر.
app-settings-error-unreadable = تعذّرت قراءة ملف الإعدادات أو حفظه.
app-settings-error-invalid = ملف الإعدادات غير صالح. افتح الإعدادات لمراجعته.
    .settings = الملف settings.json غير صالح، لذا يستخدم Ibis آخر إعدادات سليمة لديك.
app-settings-error-newer = كُتبت الإعدادات بإصدار أحدث من Ibis. حدّث Ibis قبل تغييرها.
app-settings-error-read-only = الإعدادات للقراءة فقط. اجعل ملف الإعدادات قابلًا للكتابة قبل إعادة المحاولة.
app-command-output-not-saved = تعذّر حفظ اختيار التنسيق.
app-command-delivery-not-saved = حُفظ التنسيق، لكن تعذّر حفظ اختيار اللصق.

app-command-page-missing = المرجع { $number }: يحتاج { $label } إلى صفحة أو نطاق. أدخله بعد التسمية، أو امسح { $label }.
app-command-page-invalid = المرجع { $number }: يحتاج { $label } إلى صفحة أو نطاق صالح بعد التسمية. صحّحه، أو امسح { $label }.
app-command-locator-missing = المرجع { $number }: يحتاج { $label } إلى قيمة. أدخلها بعد التسمية، أو امسح الموضع.
app-command-locator-invalid = المرجع { $number }: يحتاج { $label } إلى قيمة صالحة. صحّحها، أو امسح الموضع.
app-reference-number = المرجع { $number }
app-command-locator-conflict = المرجع { $number }: لا يمكن الجمع بين موضع مكتوب وموضع ملتقط.
app-command-bibliography-unsupported = لا يستطيع هذا النمط كتابة هذه المراجع: { $references }. أزلها من هذه القائمة للمتابعة.

## MCP validation facts.
mcp-error-arguments = يجب أن تكون arguments كائنًا.
mcp-error-query = يجب أن تكون query كلمات قليلة: اسم عائلة مؤلف، أو كلمات من العنوان، أو سنة، أو مفتاح.
mcp-error-shorthand = يجب أن تكون shorthand إما true أو false.
mcp-error-reference = يجب أن تكون ref مرجعًا من search، كنص.
mcp-error-items = يجب أن تكون items قائمة كائنات، لكل منها ref من search.
mcp-error-app = يجب أن تكون for_app معرّف حزمة macOS أو اسم ملف تنفيذي في Windows.
mcp-error-keep-order = يجب أن تكون keep_order إما true أو false.
mcp-error-output-choice = أعطِ format أو style، لا كليهما.
mcp-error-style = يجب أن تكون style معرّف نمط، كنص.
mcp-error-locale = يجب أن تكون locale وسم لغة، كنص، مثل de-DE.
mcp-error-key-locale = تنطبق locale على الأنماط فقط؛ أما صيغة المفاتيح فتترك اللغة لمعالج المستند.
mcp-error-form = يجب أن تكون form إحدى القيم في مخطط إدخال هذه الأداة.
mcp-error-records-or-items = أعطِ records أو items، لا كليهما؛ وتحتاج for_app إلى items.
mcp-error-records-order = تحتاج keep_order إلى items؛ ولا تُدعم مع records.
mcp-error-records = يجب أن تكون records قائمة كائنات CSL-JSON، لكل منها id وtype.
mcp-error-item = يجب أن يكون كل item كائنًا فيه ref من search.
mcp-error-locator = الموضع غير مقروء؛ اكتبه مثل «p. 12» أو «ch. 3» أو «12-14».
mcp-error-omit-author = يجب أن تكون omit_author إما true أو false.
mcp-error-empty-output = لم ينتج هذا الإخراج أي نص لهذه العناصر؛ جرّب شكلًا أو تنسيقًا أو نمطًا آخر.
mcp-error-style-query = يجب أن تكون query نصًا لا يتجاوز 1024 بايت.
mcp-error-limit = يجب أن تكون limit عددًا صحيحًا لا يقل عن 1؛ والحد الأقصى للنتائج المعادة { $limit }.
mcp-error-unknown-argument = ليس لـ { $tool } وسيط باسم «{ $name }»؛ وسائطه: { $names }.
mcp-error-format = يجب أن تكون format إحدى القيم: { $formats }.
mcp-error-text = يجب أن تكون { $name } نصًا.
mcp-error-number-range = يجب أن تكون { $name } عددًا صحيحًا من 1 إلى { $limit }.
mcp-error-number-minimum = يجب أن تكون { $name } عددًا صحيحًا لا يقل عن 0.
mcp-error-include-receipt = يجب أن تكون include_receipt إما true أو false.
mcp-error-stale-reference = تغيّر هذا المرجع منذ أن أعاده search؛ ابحث مجددًا وتأكد من أنه المرجع المقصود.
mcp-error-attachment-text = لم يسمح المستخدم لتطبيقات الذكاء الاصطناعي بقراءة نص المرفقات من هذه المكتبة. أخبره بذلك؛ يمكنه السماح بها لهذه المكتبة في إعدادات Ibis.
mcp-error-no-library = لا توجد مكتبة مفتوحة. اطلب من المستخدم اختيار مكتبة في Ibis، أو تشغيل `ibis mcp` مع --file أو --zotero، ثم ابدأ جلسة جديدة.
mcp-error-writes = بدأ هذا الخادم دون --allow-writes، لذا لا يستطيع اقتراح تغييرات. أخبر المستخدم؛ يمكنه إضافة --allow-writes إلى الأمر ibis mcp في تطبيق الذكاء الاصطناعي لديه.
mcp-error-proposal = لم يسجّل Ibis هذا الاقتراح: { $reason }
mcp-error-strict = { mcp-strict-help }
mcp-error-hidden-citation = { mcp-citation-hidden }
mcp-item-error = العنصر { $number }: { $reason }
mcp-item-name = العنصر { $number }
mcp-output-name = هذا الإخراج
mcp-tool-error = خطأ: { $code }. { $reason }
mcp-error-busy = Ibis مشغول. أعد هذا الاستدعاء بعد لحظة.

cli-key-suggestions = هل تقصد { $keys }؟
cli-key-not-found = لا يوجد مرجع بالمفتاح «{ $key }»{ $choices }
    جرّب: { $command }{ $route }

mcp-startup-failed = تعذّر فتح المكتبة عند بدء هذه الجلسة. { $error } عالج المشكلة، ثم ابدأ جلسة MCP جديدة.

app-bibliography-unavailable = تعذّر إعداد قائمة المراجع. حاول مرة أخرى.
app-bibliography-not-saved = تعذّر حفظ قائمة المراجع. حاول مرة أخرى.
app-bibliography-name-taken = يوجد ملف بهذا الاسم هناك. اختر اسمًا آخر.
app-bibliography-format = احفظها بصيغة ‎.rtf أو ‎.html أو ‎.txt.
app-clipboard-unavailable = تعذّرت الكتابة إلى الحافظة. حاول مرة أخرى.

app-recent-history-not-saved = تعذّر نسيان الاستشهادات الأخيرة. حاول مرة أخرى.
app-project-citation-conflict = مفاتيح الاستشهاد { $keys } تشير إلى أعمال مختلفة في قائمة المراجع. تحقق من قائمة المراجع قبل النسخ.

app-settings-error-changed = تغيّرت الإعدادات أثناء الحفظ. حاول مرة أخرى.
app-settings-error-busy = نسخة أخرى من Ibis تحفظ الإعدادات. انتظر حتى تنتهي، ثم حاول مرة أخرى.
app-settings-error-document-format = اختر نمطًا للمستندات.
app-settings-error-key-format = اختر تنسيق مفاتيح لـ LaTeX وMarkdown وObsidian.
app-project-error-context-unavailable = تعذّرت قراءة قائمة مراجع المستند ونمط الاستشهاد فيه. حاول مرة أخرى.
app-project-error-invalid-context = حدّد في المستند قائمة مراجع واحدة ونمط CSL واحدًا كقيم بسيطة.
app-project-error-ambiguous-context = أكثر من مشروع يطابق قائمة المراجع ونمط الاستشهاد هذين. اختر مشروعًا باسمه.
mcp-error-citation-format = يصدّر هذا التنسيق سجلات أو علامات Zotero. اختر تنسيق استشهاد.

app-failure-open-settings-file = تعذّر فتح ملف الإعدادات. حاول مرة أخرى.
## Command-line requests, diagnostics and reports.

cli-keys-unreadable = تعذّرت قراءة قائمة المفاتيح؛ أدخل نصًا بترميز UTF-8 بمفتاح واحد في كل سطر.
cli-keys-too-large = قائمة المفاتيح أكبر من 1 MiB؛ أرسل مفاتيح أقل في كل مرة.
cli-keys-not-utf8 = يجب أن تكون قائمة المفاتيح نصًا بترميز UTF-8، بمفتاح واحد في كل سطر.
cli-keys-empty = قائمة المفاتيح فارغة؛ أدخل مفتاح استشهاد واحدًا على الأقل.
cli-key-file-unavailable = ملف المفاتيح مفقود أو غير قابل للقراءة أو ليس ملفًا عاديًا؛ تحقق من مساره، أو استخدم - لقراءة stdin.
cli-key-file-unopened = تعذّر فتح ملف المفاتيح؛ تحقق من مساره وأذوناته.
cli-key-file-not-regular = يحتاج --keys-from إلى ملف عادي، أو - لقراءة stdin.
cli-key-file-timeout = تعذّرت قراءة ملف المفاتيح في الوقت المحدد؛ حاول مرة أخرى.
cli-toggle-unprepared = تعذّر تجهيز التبديل؛ تحقق من إمكانية الكتابة في المجلد المؤقت، ثم حاول مرة أخرى.
cli-app-libraries-unavailable = الخيار --app-libraries غير متاح هنا؛ حدّد المكتبات المراد قراءتها.
cli-records-with-libraries = يستشهد --records بالسجلات الواردة من stdin فقط؛ احذف خيارات المكتبة.
cli-search-words-needed = أدخل كلمات للبحث: مؤلفًا أو كلمات من العنوان أو سنة أو مفتاحًا.
cli-search-words-too-long = كلمات البحث طويلة جدًا؛ استخدم كلمات أقل.
cli-id-not-found = لا توجد مكتبة مفتوحة تحتوي على المرجع الذي يحدده --id.
cli-attachment-not-found = لا توجد مكتبة مفتوحة تحتوي على هذا المرفق، أو تغيّر مرجعه؛ شغّل ibis outline مجددًا للحصول على معرّف جديد.
cli-find-in-words = أدخل الكلمات المراد إيجادها، بحد أقصى 200 حرف.
cli-records-too-large = السجلات الواردة من stdin أكبر من 1 MiB؛ أرسل سجلات أقل في كل مرة.
cli-bibliography-needs-style = يحتاج --bibliography إلى نمط: حدّد --style، أو قيمة --format تكون نمطًا.
cli-project-too-many-keys = يمكن التحقق من 1000 مفتاح كحد أقصى في المرة الواحدة.
cli-styles-limit = يجب أن تكون قيمة --limit بين 1 و20000.
cli-request-help = يجب أن يحتوي stdin على كائن JSON واحد من نوع CitationRequest؛ انظر: ibis schema cite
cli-records-help = يجب أن يحتوي stdin على مصفوفة JSON واحدة من سجلات CSL-JSON؛ انظر: ibis schema records
cli-id-help = يجب أن تكون قيمة --id معرّف مرجع، أي JSON الذي يطبعه ibis search --json.
cli-key-option-owner = يجب أن يأتي --{ $flag } بعد المفتاح الذي يخصه.
cli-key-option-repeated = الخيار --{ $flag } مذكور مرتين للمفتاح «{ $key }».
cli-locator-unreadable = القيمة «{ $locator }» في --locator ليست موضعًا يقرؤه Ibis؛ اكتبها مثل «p. 12» أو «ch. 3» أو «12-14».
cli-method-unknown = لا توجد طريقة باسم «{ $method }»؛ اختر واحدة من: { $names }
cli-style-unknown = لا يوجد نمط باسم «{ $style }»؛ اعرض الأنماط باستخدام: ibis styles
cli-locale-unknown = لا توجد لغة محلية باسم «{ $locale }»؛ اعرضها باستخدام: ibis styles --locales
cli-params-help = يجب أن يكون --params معاملات JSON الخاصة بـ { $method }؛ انظر: ibis schema { $method }
cli-source-findings = تنبيه: لدى إحدى المكتبات تحذيرات أو تعذّر تحديثها؛ انظر: ibis libraries{ $libraries }{ $json }
cli-keys-waiting = في انتظار المفاتيح، مفتاح في كل سطر؛ اضغط Ctrl-D للإنهاء.
cli-search-keyless =
    { $plural ->
        [zero] تنبيه: عدد النتائج بلا مفتاح استشهاد: { $count }؛ انظر ibis search { $query }{ $libraries } --json
        [one] تنبيه: نتيجة واحدة بلا مفتاح استشهاد؛ انظر ibis search { $query }{ $libraries } --json
        [two] تنبيه: نتيجتان بلا مفتاح استشهاد؛ انظر ibis search { $query }{ $libraries } --json
        [few] تنبيه: { $count } نتائج بلا مفتاح استشهاد؛ انظر ibis search { $query }{ $libraries } --json
        [many] تنبيه: { $count } نتيجةً بلا مفتاح استشهاد؛ انظر ibis search { $query }{ $libraries } --json
       *[other] تنبيه: عدد النتائج بلا مفتاح استشهاد: { $count }؛ انظر ibis search { $query }{ $libraries } --json
    }
cli-search-more =
    { $plural ->
        [zero] تنبيه: تُعرض أول { $count } من النتائج؛ اعرض المزيد باستخدام --limit { $next }
        [one] تنبيه: تُعرض أول نتيجة فقط؛ اعرض المزيد باستخدام --limit { $next }
        [two] تنبيه: تُعرض أول نتيجتين فقط؛ اعرض المزيد باستخدام --limit { $next }
        [few] تنبيه: تُعرض أول { $count } نتائج؛ اعرض المزيد باستخدام --limit { $next }
        [many] تنبيه: تُعرض أول { $count } نتيجةً؛ اعرض المزيد باستخدام --limit { $next }
       *[other] تنبيه: تُعرض أول { $count } من النتائج؛ اعرض المزيد باستخدام --limit { $next }
    }
cli-search-narrow = تنبيه: تُعرض أول { $count } من النتائج؛ استخدم كلمات بحث أكثر تمييزًا
cli-file-name-invalid = تعذّر فتح { $path }: اسمه ليس نصًا صالحًا؛ أعد تسميته.
-cli-name-library = حدّد مكتبة باستخدام --file PATH أو --zotero أو --zotero-group ID
cli-app-settings-timeout = تعذّرت قراءة إعدادات تطبيق Ibis في الوقت المحدد؛ حاول مرة أخرى، أو { -cli-name-library }.
cli-chosen-library-unopened = تعذّر فتح المكتبة المختارة في { $app }: { $reason } اخترها مجددًا هناك، أو { -cli-name-library }.
cli-app-settings-missing = لم تُحدَّد أي مكتبة، وليس لتطبيق Ibis إعدادات هنا؛ { -cli-name-library }.
cli-app-formats-unreadable = تعذّرت قراءة التنسيقات المختارة في { $app }: { $reason }. افتح { $app } لإصلاح إعداداته.
cli-app-libraries-unreadable = تعذّرت قراءة المكتبات المختارة في { $app }: { $reason }. { $fix }، أو { -cli-name-library }.
cli-app-library-unchosen = لم تُختر أي مكتبة في { $app } بعد؛ اختر واحدة هناك، أو { -cli-name-library }.
cli-app-library-relative = المكتبة المختارة في { $app } ليست مسارًا كاملًا؛ اخترها مجددًا هناك، أو { -cli-name-library }.
cli-app-settings-changed = تغيّرت إعداداته أثناء الحفظ؛ حاول مرة أخرى
cli-app-settings-permission = هذا البرنامج غير مسموح له بقراءة ملف إعداداته؛ اسمح له بذلك، أو شغّله خارج بيئة العزل
cli-app-settings-file-missing = ملف إعداداته مفقود
cli-app-settings-unreadable = تعذّرت قراءة ملف إعداداته
cli-app-settings-invalid = ملف إعداداته غير صالح
cli-app-settings-backup-unused = { $reason }، ولا يستخدم Ibis نسخته الاحتياطية هنا
cli-app-settings-update = حدّث أمر ibis هذا ليطابق { $app }
cli-app-settings-repair = افتح { $app } لإصلاح إعداداته
cli-app-settings-zotero-invalid = ملف إعداداته غير صالح (مُدخل مكتبة Zotero رقم { $position })
cli-app-settings-newer = كتب إعداداته إصدار أحدث من Ibis (التنسيق { $version })
cli-settings-no-folder = لا يوجد على هذا الكمبيوتر مجلد لإعدادات التطبيقات (لم يُضبط HOME أو APPDATA).
cli-settings-action-needed = اختر export أو import؛ انظر: ibis settings --help
cli-settings-backup-needed = حدّد ملف النسخة الاحتياطية المراد استعادته.
cli-settings-folder-timeout = لم يستجب مجلد الإعدادات في الوقت المحدد؛ حاول مرة أخرى.
cli-settings-exported = حُفظت نسخة احتياطية من إعدادات { $app } في { $name }.
    تبقى موافقات اللصق ومواضع النوافذ على هذا الكمبيوتر.
cli-settings-backup-made = نسخة احتياطية أنشأها Ibis { $version } على { $system }
cli-settings-unchanged = لدى { $app } الإعدادات الموجودة في { $backup } بالفعل. لم يتغير شيء.
cli-settings-would-restore = ستؤدي استعادة { $backup } إلى تغيير إعدادات { $app }:
cli-settings-restored = تمت استعادة { $backup } إلى { $app }:
cli-settings-library-not-found = تنبيه: ملف المكتبة في النسخة الاحتياطية غير موجود على هذا الكمبيوتر. يُحتفظ به، وسيجده Ibis متى توفر الملف.
cli-settings-shortcut-kept = تنبيه: لا تتضمن النسخة الاحتياطية اختصارًا لفتح Ibis على هذا النظام، لذا يبقى اختصار هذا الكمبيوتر.
cli-settings-repairs-settings = تنبيه: ملف الإعدادات على هذا الكمبيوتر تالف أو مفقود. الاستعادة تكتبه من جديد، وتحتفظ بأي ملف تالف بجانبه.
cli-settings-dry-run = لم يتغير شيء. شغّله مرة أخرى دون --dry-run للاستعادة.
cli-settings-restored-next = يُحتفظ بالإعدادات المستبدَلة في settings.json.bak. تبقى موافقات اللصق ومواضع النوافذ كما كانت.
    يستخدم Ibis الإعدادات المستعادة في المرة القادمة التي يُفتح فيها؛ أنهِه وأعد فتحه إن كان قيد التشغيل.
cli-writing-style-unavailable = نمط الاستشهاد هذا غير متاح. اعرض الأنماط باستخدام: ibis styles
cli-writing-output-unavailable = هذا الإخراج غير متاح. اعرض أنواع الإخراج باستخدام: ibis outputs
cli-writing-bibliography-needed = اختر قائمة مراجع BibTeX أو BibLaTeX باسم ملف ينتهي بـ ‎.bib.
cli-writing-folder-unavailable = المجلد الحالي غير متاح. أدخل المسار الكامل لقائمة المراجع.
cli-writing-action-needed = اختر list أو create أو rename أو update أو remove.
cli-writings-empty = لا توجد مشاريع بعد. أنشئ واحدًا باستخدام: ibis projects create NAME --format pandoc
cli-writing-format-needed = اختر --format أو --style.
cli-writing-citation-unavailable = استشهاد المشروع غير متاح.
cli-writing-saved =
    { $action ->
        [create] تم إنشاء المشروع { $name }.
        [rename] تمت إعادة تسمية المشروع { $name }.
        [remove] تمت إزالة المشروع { $name }. واحتُفظ بملف قائمة المراجع الخاص به.
       *[update] تم تحديث المشروع { $name }.
    }
cli-no-library = لا توجد مكتبة مفتوحة؛ أضف --file <library> أو --zotero.
cli-search-no-match-terminal = لا توجد مراجع مطابقة. جرّب بحثًا أقصر.
cli-search-no-match = لم يُعثر على مراجع.
cli-libraries-empty = لم تُحمَّل أي مكتبات. افتح واحدة باستخدام --file <library>.
cli-styles-no-match = لا يوجد نمط يطابق هذه الكلمات.
cli-share-written =
    { $count ->
        [zero] كُتب: لا ملفات، { $size }، إلى { $path }.
        [one] كُتب: ملف واحد، { $size }، إلى { $path }.
        [two] كُتب: ملفان، { $size }، إلى { $path }.
        [few] كُتب: { $count } ملفات، { $size }، إلى { $path }.
        [many] كُتب: { $count } ملفًا، { $size }، إلى { $path }.
       *[other] كُتب: { $count } ملف، { $size }، إلى { $path }.
    }
cli-share-preview =
    { $count ->
        [zero] لا ملفات، { $size }. أضف --to PATH لكتابة الحزمة.
        [one] ملف واحد، { $size }. أضف --to PATH لكتابة الحزمة.
        [two] ملفان، { $size }. أضف --to PATH لكتابة الحزمة.
        [few] { $count } ملفات، { $size }. أضف --to PATH لكتابة الحزمة.
        [many] { $count } ملفًا، { $size }. أضف --to PATH لكتابة الحزمة.
       *[other] { $count } ملف، { $size }. أضف --to PATH لكتابة الحزمة.
    }
cli-share-not-in-list = تُسقط قائمة القراءة ما لا يستطيع نمطها كتابته: { $keys }
cli-share-not-in-bibtex = يُسقط References.bib ما ليس له مفتاح استشهاد صالح: { $keys }
cli-share-without-pdf = تعذّرت إضافة ملف PDF من مكتبتها لـ: { $keys }
cli-outline-none = لا يحتوي هذا المرجع على مرفقات.
cli-outline-pages = نص، عدد الصفحات: { $count }
cli-outline-printed-pages = نص، عدد الصفحات: { $count }، بأرقام صفحات مطبوعة
cli-outline-text = نص، عدد الصفحات غير معروف
cli-outline-not-indexed = لا يوجد نص: لم يفهرسه Zotero
cli-outline-no-file = لا يوجد نص: رابط بلا ملف
cli-outline-too-large = لا يوجد نص: أكبر مما يقرؤه Ibis
cli-outline-not-yet = لا يوجد نص: لا يستطيع Ibis قراءة هذا الملف بعد
cli-outline-missing = لا يوجد نص: لا يوجد ملف في المكان الذي تحدده المكتبة
cli-outline-not-local = لا يوجد نص: ليس ملفًا عاديًا على هذا الكمبيوتر، مثل رابط أو ملف متاح عبر الإنترنت فقط
cli-outline-not-pdf = لا يوجد نص: الملف ليس PDF
cli-outline-damaged = لا يوجد نص: ملف PDF تالف
cli-outline-timed-out = لا يوجد نص: استغرقت قراءة ملف PDF أكثر من عشر ثوانٍ
cli-read-page = [صفحة الملف { $page }]
cli-read-printed-page = [صفحة الملف { $page }، الصفحة المطبوعة { $label }]
cli-read-more = مزيد من النص: { $command }
cli-find-in-none = لا يوجد مقطع يحتوي على هذه الكلمات.
cli-find-in-more = أول { $count } من المقاطع؛ أضف كلمات لتضييق النتائج.
cli-entry-no-original = لا يوجد لهذا المرجع مُدخل BibTeX أصلي هنا: مكتبة ‎.bib وحدها تحتفظ به، أما مكتبات Zotero وCSL-JSON وRIS وCSV فلا. لكتابة مُدخل من حقوله، شغّل ibis cite KEY --format bibtex، أو صدّره من مدير المراجع بالصيغة التي يستخدمها مشروعك مع الاحتفاظ بمفتاحه.
cli-entry-changed = تغيّر ملف المكتبة أو تعذّرت قراءته. أعد تحميله وحاول مرة أخرى.
cli-entry-repeated = يتكرر هذا المفتاح في ملف المكتبة، لذا لا يستطيع Ibis تحديد المُدخل المطلوب.
cli-entry-dependency = يعتمد هذا المُدخل على crossref أو xdata أو @string لا يمكن إرفاقه معه. صدّره من مدير المراجع مع الاحتفاظ بمفتاحه.
cli-library-refresh-failed = تنبيه: تعذّر تحديث هذه المكتبة؛ تُعرض آخر قراءة ناجحة
cli-library-not-watched = تنبيه: لا تُرصد التغييرات تلقائيًا
cli-warning-unknown-field = تم تخطي حقل غير معروف؛ واحتفظ Ibis بالمرجع
cli-warning-unknown-type = نوع المُدخل غير مدعوم؛ يتخطى Ibis هذا المُدخل
cli-warning-duplicate-key = يستخدم أكثر من مُدخل هذا المفتاح أو المعرّف؛ يتخطاها Ibis جميعًا
cli-warning-syntax-entry = تعذّرت قراءة هذا المُدخل؛ يتخطاه Ibis
cli-warning-entry-too-large = المُدخل أكبر من أن يُقرأ؛ يتخطاه Ibis
cli-warning-library-incomplete = تعذّر على Ibis قراءة المكتبة كاملة؛ قد تكون بعض المراجع مفقودة
cli-warning-attachment-skipped = رابط أحد المرفقات غير صالح؛ يستبعده Ibis
cli-warning-syntax = تعذّرت قراءة أحد المُدخلات، لذا يتخطاه Ibis
cli-warning-field = تعذّر تحويل أحد الحقول بالكامل؛ تحقق من هذا المُدخل
cli-warning-work-limit = المكتبة أكبر من أن تُقرأ كاملة
cli-warning-invalid-attachment = رابط أحد المرفقات غير صالح
cli-warning-pdfs-not-kept = في المجلد ملفات PDF لم يحفظها Ibis بعد؛ شغّل ibis sources keep --file FOLDER
cli-warning-cloud-directory = المكتبة في مجلد متزامن مع السحابة، لذا قد تتأخر التغييرات
cli-warning-keys-unavailable = تعذّرت قراءة مفاتيح Better BibTeX، لذا بعض العناصر بلا مفاتيح
cli-warning-unknown = شيء ما في المكتبة يحتاج إلى مراجعة
cli-output-disk-full = تعذّرت كتابة الإخراج لأن القرص ممتلئ؛ أفرغ بعض المساحة وحاول مرة أخرى.
cli-output-unwritten = تعذّرت كتابة الإخراج؛ تحقق من وجهته وحاول مرة أخرى.
cli-file-busy = الملف قيد الكتابة أو تتعذّر قراءته الآن؛ حاول مرة أخرى بعد قليل.
cli-file-too-large = الملف أكبر مما يقرؤه Ibis، أو هناك ملفات كثيرة مفتوحة في وقت واحد؛ قسّم المكتبة أو افتح ملفات أقل.
cli-file-missing = لا يوجد ملف في هذا المسار؛ تحقق من الإملاء أو أدخل المسار الكامل.
cli-file-is-folder = هذا مجلد؛ حدّد ملف المكتبة الموجود داخله.
cli-file-permission = هذا البرنامج غير مسموح له بقراءة الملف؛ تحقق من أذوناته، أو شغّله خارج بيئة العزل.
cli-zotero-unavailable = لم يُعثر على مكتبة Zotero محلية أو تعذّرت قراءتها؛ تحقق من أن Zotero مثبّت وفُتح مرة واحدة على الأقل، أو استخدم --file.
cli-zotero-damaged = تعذّر فهم بياناتها؛ افتح Zotero مرة واحدة ليصلح قاعدة بياناته.
cli-zotero-newer = كتبها إصدار أحدث من Zotero؛ حدّث Ibis.
cli-zotero-too-large = حجمها أكبر مما يقرؤه Ibis دفعة واحدة.
cli-key-empty = لا يمكن أن يكون مفتاح الاستشهاد فارغًا.
cli-search-partial = لا يوجد مرجع يطابق كل الكلمات. أقرب التطابقات:
    { $rows }
cli-styles-showing = تُعرض الأنماط: { $shown } من أصل { $total }؛ ضيّق النتائج باستخدام: ibis styles WORDS
cli-project-unreadable = تعذّرت قراءة { $name }: { $reason }.
cli-project-damaged = تعذّرت قراءة { $name } بالكامل: تحقق من السطر { $lines }.
cli-warning-text-kept = تعذّر تحويل { $subject } بالكامل؛ يستخدم Ibis النص كما كُتب
cli-warning-date = تعذّر تحويل { $subject } بالكامل؛ احتفظ Ibis بالمرجع؛ تحقق من التاريخ قبل الاستشهاد
cli-warning-link = { $subject } خاص أو ليس عنوان ويب؛ يستبعده Ibis
cli-warning-names = تعذّرت قراءة { $subject } بالكامل؛ يستخدم Ibis الأسماء التي أمكن قراءتها
cli-warning-fields-kept = تعذّر تحويل { $subject } بالكامل؛ احتفظ Ibis بالمرجع مع الحقول التي أمكن قراءتها
cli-warning-syntax-field = تعذّرت قراءة { $subject }؛ يتخطى Ibis هذا المُدخل
cli-warning-duplicate-key-named = يستخدم أكثر من مُدخل «{ $key }» مفتاحًا أو معرّفًا، لذا يتخطاها Ibis جميعًا
cli-search-stale-at = تنبيه: تعذّر تحديث { $name }؛ تُعرض النتائج كما كانت في { $time }
cli-search-stale = تنبيه: تعذّر تحديث { $name }؛ تُعرض آخر قراءة ناجحة
cli-file-unopened = تعذّر فتح { $path }: { $reason }
cli-zotero-unreadable-profiles = تعذّرت قراءة مكتبة Zotero: { $reason } ملفات التعريف: { $profiles }.
cli-zotero-unreadable = تعذّرت قراءة مكتبة Zotero: { $reason }
cli-key-duplicate = يستخدم أكثر من مُدخل مفتاح الاستشهاد «{ $key }»{ $at }، لذا يتخطاها Ibis جميعًا؛ أعطِ كل مُدخل مفتاحًا خاصًا به.
cli-key-damaged = تعذّرت قراءة المُدخل «{ $key }»{ $at }، لذا يتخطاه Ibis{ $reason }؛ أصلحه في مكتبتك.
cli-key-library-incomplete = تعذّر العثور على «{ $key }»: { $reason }
cli-key-ambiguous = تستخدم عدة مراجع مفتاح الاستشهاد «{ $key }»؛ أعطِ كلًّا منها مفتاحًا خاصًا به في مكتبتك.
cli-key-without-library = تعذّر البحث عن «{ $key }»: لا توجد مكتبة مفتوحة؛ أضف --file <library> أو --zotero.
cli-key-not-found-search = لا يوجد مرجع بمفتاح الاستشهاد «{ $key }»؛ ابحث عن المفتاح الصحيح باستخدام: ibis search <words>
cli-key-too-long = مفتاح الاستشهاد «{ $key }…» طويل جدًا.
cli-project-unavailable = الملف غير موجود أو ليس على هذا الكمبيوتر بعد
cli-project-not-file = ليس ملفًا
cli-project-too-large = حجم الملف أكبر من 64 MiB
cli-project-not-utf8 = الملف ليس نصًا بترميز UTF-8
cli-project-changing = الملف لا يزال يتغيّر
cli-project-not-bibtex = يجب أن تكون قائمة المراجع ملف BibTeX أو BibLaTeX
cli-project-fields-differ = يختلف هذا المُدخل عمّا في المكتبة
cli-project-case-differs = يختلف أحد المفاتيح في حالة الأحرف فقط
cli-project-alias = إنه اسم بديل لمُدخل آخر
cli-project-helper = ينتمي إلى مُدخل @xdata، ولا يمكن الاستشهاد به
cli-project-commented = يقع داخل @comment، الذي لا تزال بعض الأدوات تقرؤه
cli-project-key-doubt = قد يشير المفتاح إلى مرجع آخر (السطر { $line }): { $reason }
cli-destination-paste = تنبيه: يلصق Ibis في { $name }.
cli-destination-unknown = تنبيه: لا يعرف Ibis هذا التطبيق، لذا يسأل قبل اللصق.
cli-destination-unsupported = تنبيه: ينسخ Ibis هنا، لأنه لا يستطيع اللصق على هذا النظام بعد.
cli-destination-terminal = تنبيه: ينسخ Ibis من أجل { $name }، لأن اللصق هناك يُنفَّذ كإدخال مكتوب.
cli-destination-remote = تنبيه: ينسخ Ibis من أجل { $name }، لأن ضغطات المفاتيح تصل هناك إلى نظام آخر.
cli-destination-shortcut = تنبيه: ينسخ Ibis من أجل { $name }، لأن مفتاح اللصق فيه يؤدي وظيفة أخرى.
cli-destination-copy = تنبيه: ينسخ Ibis من أجل { $name }.
cli-doctor-all-good = كل شيء على ما يرام.
cli-doctor-one-finding = أمر واحد يحتاج إلى مراجعة.
cli-doctor-findings = أمور تحتاج إلى مراجعة: { $count }.
cli-doctor-last-read = آخر قراءة ناجحة: { $age }.
cli-doctor-check-failed = فشل الفحص.
cli-search-shorthand = مُستشهد به: { $cited }؛ كلمات البحث: { $words }
cli-project-key-in = في { $name } (السطر { $line })
cli-project-key-missing = غير موجود في { $name }
cli-project-key-unchecked = تعذّر التحقق من { $name }
cli-project-key-repeated = يتكرر المفتاح (الأسطر { $lines })
cli-citation-language = تنبيه: لغة الاستشهاد { $language }.
cli-library-stale-note = (آخر قراءة ناجحة؛ فشل التحديث)
cli-library-watch-note = (لا تُرصد التغييرات تلقائيًا)
cli-greeting = يجد Ibis { $version } المراجع ويستشهد بها في أي مكان.

    جرّبه:
      ibis search lantern --file refs.bib       ابحث عن مرجع
      ibis cite vale2024lanterns --file refs.bib --style apa
                                                أنشئ استشهادًا
      ibis doctor --file refs.bib               افحص مكتبة

    إذا كان تطبيق Ibis مثبّتًا، يمكنك حذف --file.
    أضف --json إلى أي أمر للسكربتات والوكلاء. كل الأوامر: ibis --help

# Settings and ibis agents share these app connections.
settings-ai-apps = تطبيقات الذكاء الاصطناعي
ai-apps-command-line = سطر الأوامر
ai-apps-description = اربط Ibis بتطبيقات الذكاء الاصطناعي على هذا الكمبيوتر.
ai-apps-words = وكلاء ذكاء اصطناعي MCP Claude Codex Cursor VS Code Gemini Windsurf LM Studio ChatGPT تطبيقات مساعد
proposals-title = التغييرات المقترحة
proposals-description = راجع تغييرات المكتبة التي تقترحها تطبيقات الذكاء الاصطناعي وطبّقها.
proposals-words = مقترحات اقتراحات تغييرات إضافة مرجع
proposals-checking = جارٍ التحقق من التغييرات المقترحة…
proposals-empty = لا توجد تغييرات مقترحة
proposals-from = { $app } · { $library }
proposals-apply = تطبيق
proposals-reject = رفض
proposals-app = { $app } · المقترحات: { $count }
proposals-reject-app = رفض الكل
proposals-applied = تم التطبيق؛ وحُفظت نسخة احتياطية باسم { $name }.
proposals-failed = تعذّر إكمال هذا الإجراء. حاول مرة أخرى.
proposals-read-failed = تعذّرت قراءة التغييرات المقترحة. عد إلى هذه الصفحة للمحاولة مرة أخرى.
app-failure-waiting-proposals = { proposals-read-failed }
app-failure-apply-proposal = { proposals-failed }
app-failure-reject-proposal = { proposals-failed }
app-failure-reject-app-proposals = { proposals-failed }
command-words = سطر الأوامر طرفية terminal shell صدفة PATH وكلاء MCP
ai-apps-checking = جارٍ التحقق من تطبيقات الذكاء الاصطناعي…
ai-apps-not-connected = غير متصل.
ai-apps-connected = متصل. ابدأ جلسة جديدة في التطبيق لاستخدام Ibis.
ai-apps-stale = نُقل أمر ibis أو أنه مفقود.
ai-apps-different = لدى هذا التطبيق اتصال مختلف بـ Ibis. راجع التغيير أدناه.
ai-apps-plugin = متصل عبر إضافة Ibis.
ai-apps-connect = اتصال
ai-apps-disconnect = قطع الاتصال
ai-apps-repair = إصلاح
ai-apps-replace = استبدال…
ai-apps-command-needed = أمر ibis مفقود. ثبّته من «سطر الأوامر» أعلاه ليتمكن هذا التطبيق من تشغيل Ibis.
ai-apps-failed = تعذّر تغيير اتصال تطبيق الذكاء الاصطناعي. تحقق من إعدادات التطبيق وأذونات المجلد، ثم حاول مرة أخرى.
ai-apps-kept = تبقى الخوادم والإعدادات الأخرى كما هي. وتُحفظ نسخة احتياطية بجانب كل ملف مُعدَّل، كما يفعل ibis agents في الطرفية.
app-failure-agent-apps = { ai-apps-failed }
app-failure-connect-agent-app = { ai-apps-failed }
app-failure-disconnect-agent-app = { ai-apps-failed }
